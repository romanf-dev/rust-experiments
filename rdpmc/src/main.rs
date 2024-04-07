use std::arch::asm;
use std::time::Duration;
use std::thread;

fn rdpmc(counter: i32) -> u64 {
    let mut output: u64 = 0;
    unsafe {
        asm!(
            "mov ecx, {1:e}",
            "rdpmc",
            "shl rdx, 32",
            "or rax, rdx",
            "mov {0}, rax",
            out(reg) output,
            in(reg) counter,
        );
    }
    output
}

fn main() {
    let mut c1 = rdpmc(1);
    let mut c2 = rdpmc(2);
    
    for _ in 0..20 {
        let x1 = rdpmc(1);
        let x2 = rdpmc(2);
        let delta1 = x1 as f64 - c1 as f64;
        let delta2 = x2 as f64 - c2 as f64;
        c1 = x1;
        c2 = x2;
        println!("Test: {}", delta1 / delta2);
        thread::sleep(Duration::from_millis(1000));
    }
}
