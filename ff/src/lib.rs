use nix::sys::utsname::uname;
use statistical::{mean, median, standard_deviation};

pub mod fs;
pub mod mount;

pub fn summary(mut samples_ns: Vec<f64>) {
    println!("=> generating summary");

    let mu = mean(&samples_ns);
    let med = median(&samples_ns);
    let sd = standard_deviation(&samples_ns, Some(mu));

    // calculate percentiles and min/max.
    samples_ns.sort_by(|a, b| a.total_cmp(b));
    let pct = |p: f64| -> f64 {
        let n = samples_ns.len();
        if n == 1 {
            return samples_ns[0];
        }
        let p = p.clamp(0.0, 100.0);
        let rank = (p / 100.0) * ((n - 1) as f64);
        let lo = rank.floor() as usize;
        let hi = rank.ceil() as usize;
        let frac = rank - lo as f64;
        if lo == hi {
            samples_ns[lo]
        } else {
            samples_ns[lo] * (1.0 - frac) + samples_ns[hi] * frac
        }
    };

    println!();
    println!("mean   : {mu:.2} ns");
    println!("median : {med:.2} ns");
    println!("stdev  : {sd:.2} ns");
    println!("p50    : {:.2} ns", pct(50.0));
    println!("p90    : {:.2} ns", pct(90.0));
    println!("p99    : {:.2} ns", pct(99.0));
    println!(
        "min/max: {:.2} / {:.2} ns",
        samples_ns.first().unwrap(),
        samples_ns.last().unwrap()
    );
}

pub struct KernelVersion {
    major: u32,
    minor: u32,
    #[allow(dead_code)]
    patch: u32,
}

impl KernelVersion {
    pub fn current() -> Self {
        let release = uname().expect("should be able to uname");
        let release = release.release().to_string_lossy();
        let mut parts = release.split(['.', '-']);

        let major = parts.next().unwrap().parse().unwrap();
        let minor = parts.next().unwrap().parse().unwrap();
        let patch = parts.next().unwrap().parse().unwrap();

        KernelVersion {
            major,
            minor,
            patch,
        }
    }

    pub fn at_least(&self, maj: u32, min: u32) -> bool {
        (self.major, self.minor) >= (maj, min)
    }
}
