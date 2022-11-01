#![no_std]
#![no_main]
#![feature(inline_const_pat)]
#![allow(incomplete_features)]

use core::arch::asm;

/// This is how many columns the hex editor has. Align stuff to this
const ALIGN: usize = 32;

#[panic_handler]
fn ph(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

/// A region with a start subtracted from the stack pointer
struct NegOffsetedRegion {
    off: usize,
    len: usize,
}

// Put buffers high up the stack where they don't interfere with anything else
const PLAYFIELD_REGION: NegOffsetedRegion = NegOffsetedRegion {
    off: 100_000,
    len: ALIGN * ALIGN,
};

impl NegOffsetedRegion {
    unsafe fn make_slice_from_sp(&self, stack_ptr: *mut u8) -> &'static mut [u8] {
        let mut ptr = stack_ptr.sub(self.off);
        // Align to hex editor width
        while (ptr as usize) % ALIGN != 0 {
            ptr = ptr.sub(1);
        }
        core::slice::from_raw_parts_mut(ptr, self.len)
    }
}

type Level<'a> = &'a [u8];

const LEVEL_0: Level = b"\
########
#      #
#@ $   #
#   $  #
# . .  #
########
";
const LEVEL_1: Level = b"\
########
#     .#
#@ $   #
#   $  #
# . *  #
########
";
const LEVEL_2: Level = b"\
######
#.   #
#    #
###$ .##
#   #.#
# $$$.#
#@ .$ #
#######
";

const LEVELS: [Level; 3] = [LEVEL_0, LEVEL_1, LEVEL_2];

#[derive(Default)]
struct Game {
    level_idx: usize,
}

fn load_level(lvl: &[u8], playfield: &mut [u8]) {
    let ptr = playfield.as_mut_ptr();
    let mut x = 0;
    let mut y = 0;

    for &byte in lvl {
        let ch = match byte {
            b'\n' => {
                x = 0;
                y += 1;
                continue;
            }
            b'p' => TILES.pusher,
            b'P' => TILES.pusher_on_goal,
            b'b' => TILES.box_,
            b'B' => TILES.box_on_goal,
            b'#' => TILES.wall,
            b' ' | b'-' | b'_' => TILES.floor,
            c => c,
        };
        unsafe {
            ptr.add(y * ALIGN + x).write_volatile(ch);
        }
        //playfield[y * ALIGN + x] = ch;
        x += 1;
    }
}

struct Chars {
    wall: u8,
    pusher: u8,
    pusher_on_goal: u8,
    box_: u8,
    box_on_goal: u8,
    goal: u8,
    floor: u8,
}

const TILES: Chars = Chars {
    wall: 0xFF,
    pusher: b'@',
    pusher_on_goal: b'+',
    box_: b'$',
    box_on_goal: b'*',
    goal: b'.',
    floor: 0,
};

impl Game {
    fn level_start(&mut self, playfield: &mut [u8]) {
        load_level(LEVELS[self.level_idx], playfield);
    }
    fn update(&mut self, playfield: &mut [u8], input: u8) {
        let player_idx = playfield
            .iter()
            .position(|&b| b == TILES.pusher)
            .unwrap_or(0);
        enum Dir {
            Up,
            Down,
            Left,
            Right,
        }
        let (dir, new_idx) = match input {
            b'w' => (Dir::Up, player_idx - ALIGN),
            b'd' => (Dir::Right, player_idx + 1),
            b's' => (Dir::Down, player_idx + ALIGN),
            b'a' => (Dir::Left, player_idx - 1),
            b'r' => {
                self.level_start(playfield);
                return;
            }
            _ => return,
        };
        let obj = playfield[new_idx];
        match obj {
            const { TILES.floor } => playfield.swap(player_idx, new_idx),
            const { TILES.box_ } => {
                let box_idx = new_idx;
                let box_new_idx = match dir {
                    Dir::Up => box_idx - ALIGN,
                    Dir::Down => box_idx + ALIGN,
                    Dir::Left => box_idx - 1,
                    Dir::Right => box_idx + 1,
                };
                match playfield[box_new_idx] {
                    const {TILES.floor} => playfield.swap(box_idx, box_new_idx),
                    const {TILES.goal} => {
                        playfield[box_idx] = TILES.floor;
                        playfield[box_new_idx] = TILES.box_on_goal;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        // Win condition: No empty goals
        if !playfield.into_iter().any(|b| *b == TILES.goal) {
            self.level_idx += 1;
            load_level(LEVELS[self.level_idx], playfield);
        }
    }
}

unsafe fn ui_ptr(stack_ptr: *mut u8) -> *mut u8 {
    stack_ptr
        .sub(PLAYFIELD_REGION.off)
        .add(PLAYFIELD_REGION.len)
}

#[no_mangle]
unsafe extern "C" fn _start() {
    let mut stack_ptr: *mut u8;
    asm! {"mov {}, rsp", out(reg) stack_ptr };
    let playfield = PLAYFIELD_REGION.make_slice_from_sp(stack_ptr);
    let mut game = Game::default();
    game.level_start(playfield);
    ui_ptr(stack_ptr).write_volatile(0xff);
    loop {
        // Wait for input
        while ui_ptr(stack_ptr).read_volatile() == 0xff {}
        // Update
        let input = ui_ptr(stack_ptr).read_volatile();
        game.update(playfield, input);
        // Reset input to waiting state
        ui_ptr(stack_ptr).write_volatile(0xff);
    }
}
