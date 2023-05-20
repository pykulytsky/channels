use std::arch::asm;
use std::sync::atomic::AtomicUsize;

#[cfg(not(target_os = "linux"))]
compile_error!("Linux only");

#[inline]
pub unsafe fn syscall4(n: u32, arg1: *const AtomicUsize, arg2: usize, arg3: usize) -> usize {
    let mut ret: usize;
    asm!(
        "syscall",
        inlateout("rax") n as usize => ret,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        out("rcx") _, // rcx is used to store old rip
        out("r11") _, // r11 is used to store old rflags
        options(nostack, preserves_flags)
    );
    ret
}

#[inline]
pub unsafe fn syscall5(
    n: u32,
    arg1: *const AtomicUsize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
) -> usize {
    let mut ret: usize;
    asm!(
        "syscall",
        inlateout("rax") n as usize => ret,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        in("r10") arg4,
        out("rcx") _, // rcx is used to store old rip
        out("r11") _, // r11 is used to store old rflags
        options(nostack, preserves_flags)
    );
    ret
}

#[inline]
pub fn wait(a: &AtomicUsize, expected: usize) {
    unsafe {
        syscall5(202, a as *const AtomicUsize, 0, expected, 0);
    }
}

#[inline]
pub fn wake_one(a: &AtomicUsize) {
    unsafe {
        syscall4(202, a as *const AtomicUsize, 1, 1);
    }
}
