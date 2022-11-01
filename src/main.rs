#![no_std]
#![no_main]
#![feature(inline_const_pat)]
#![allow(incomplete_features)]

/// This is how many columns the hex editor has. Align stuff to this
const ALIGN: usize = 32;

#[panic_handler]
fn ph(_info: &core::panic::PanicInfo) -> ! {
    unsafe { die(0xff) }
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

#[inline(always)]
unsafe fn load_level() {
    let lvl_idx = mem_read(LEVEL_OFFS) as usize;
    let lvl = match lvl_idx {
        0 => LEVEL_0,
        1 => LEVEL_1,
        2 => LEVEL_2,
        _ => die(2),
    };
    mem_playfield_clear();
    let mut x = 0;
    let mut y = 0;

    for i in 0..lvl.len() {
        let byte = lvl.as_ptr().add(i).read_volatile();
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
        mem_write(y * ALIGN + x, ch);
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

#[inline(always)]
unsafe fn mem_swap(idx1: usize, idx2: usize) {
    let tmp = MEM.as_ptr().add(idx1).read_volatile();
    MEM.as_mut_ptr()
        .add(idx1)
        .write_volatile(MEM.as_ptr().add(idx2).read_volatile());
    MEM.as_mut_ptr().add(idx2).write_volatile(tmp);
}

#[inline(always)]
unsafe fn mem_read(idx: usize) -> u8 {
    MEM.as_ptr().add(idx).read_volatile()
}

#[inline(always)]
unsafe fn mem_write(idx: usize, val: u8) {
    MEM.as_mut_ptr().add(idx).write_volatile(val)
}

/// Clear with a nice recognizable byte
#[inline(always)]
unsafe fn mem_playfield_clear() {
    for i in 0..PLAYFIELD_END {
        MEM.as_mut_ptr().add(i).write_volatile(0xCC);
    }
}

#[inline(always)]
unsafe fn die(code: u8) -> ! {
    mem_write(ERR_CODE_OFFS, code);
    loop {}
}

unsafe fn tile_leave(composite: u8) -> u8 {
    match composite {
        const { TILES.box_on_goal } | const { TILES.pusher_on_goal } => TILES.goal,
        const { TILES.box_ } | const { TILES.pusher } => TILES.floor,
        _ => die(1),
    }
}

#[inline(always)]
unsafe fn update() {
    let input: u8 = mem_read(INPUT_OFFS);
    let mut pusher_idx = 0;
    for i in 0..PLAYFIELD_END {
        let t = mem_read(i);
        if t == TILES.pusher || t == TILES.pusher_on_goal {
            pusher_idx = i;
        }
    }
    if pusher_idx == 0 {
        die(4);
    }
    enum Dir {
        Up,
        Down,
        Left,
        Right,
    }
    let (dir, new_idx) = match input {
        b'w' => (Dir::Up, pusher_idx - ALIGN),
        b'd' => (Dir::Right, pusher_idx + 1),
        b's' => (Dir::Down, pusher_idx + ALIGN),
        b'a' => (Dir::Left, pusher_idx - 1),
        b'r' => {
            load_level();
            return;
        }
        _ => return,
    };
    let old_tile = mem_read(pusher_idx);
    let new_tile = mem_read(new_idx);
    match new_tile {
        const { TILES.box_ } | const { TILES.box_on_goal } => {
            let box_idx = new_idx;
            let old_tile = mem_read(box_idx);
            let box_new_idx = match dir {
                Dir::Up => box_idx - ALIGN,
                Dir::Down => box_idx + ALIGN,
                Dir::Left => box_idx - 1,
                Dir::Right => box_idx + 1,
            };
            match mem_read(box_new_idx) {
                const { TILES.floor } => {
                    mem_write(box_idx, tile_leave(old_tile));
                    mem_write(box_new_idx, TILES.box_);
                }
                const { TILES.goal } => {
                    mem_write(box_idx, tile_leave(old_tile));
                    mem_write(box_new_idx, TILES.box_on_goal);
                }
                _ => {}
            }
        }
        const { TILES.wall } => {}
        const { TILES.floor } => {
            mem_write(pusher_idx, tile_leave(old_tile));
            mem_write(new_idx, TILES.pusher);
        }
        const { TILES.goal } => {
            mem_write(pusher_idx, tile_leave(old_tile));
            mem_write(new_idx, TILES.pusher_on_goal);
        }
        _ => die(3),
    }
    // Win condition: No empty goals
    let mut win = true;
    for i in 0..PLAYFIELD_END {
        let tile = mem_read(i);
        if tile == TILES.goal || tile == TILES.pusher_on_goal {
            win = false;
        }
    }
    if win {
        mem_write(LEVEL_OFFS, mem_read(LEVEL_OFFS) + 1);
        load_level();
    }
}

#[no_mangle]
unsafe extern "C" fn _start() {
    mem_write(INPUT_OFFS, INPUT_NONE);
    mem_write(ERR_CODE_OFFS, 0x42); // 42 = ok, lol, get it?
    mem_write(LEVEL_OFFS, 0);
    load_level();
    loop {
        // Wait for input
        while mem_read(INPUT_OFFS) == INPUT_NONE {}
        // Update
        update();
        // Reset input to waiting state
        mem_write(INPUT_OFFS, INPUT_NONE);
    }
}

const INPUT_NONE: u8 = b' ';

const MEM_SIZE: usize = ALIGN * 10;
const INPUT_OFFS: usize = MEM_SIZE - 1;
const LEVEL_OFFS: usize = MEM_SIZE - 2;
const ERR_CODE_OFFS: usize = MEM_SIZE - 3;
const PLAYFIELD_END: usize = MEM_SIZE - 4;

static mut MEM: [u8; MEM_SIZE] = [0; MEM_SIZE];
