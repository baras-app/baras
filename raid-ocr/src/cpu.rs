//! CPU details included in raid detection logs.

use std::sync::OnceLock;

/// CPU brand, SIMD level, and thread count.
pub fn summary() -> &'static str {
    static SUMMARY: OnceLock<String> = OnceLock::new();
    SUMMARY.get_or_init(build_summary)
}

fn build_summary() -> String {
    let threads = std::thread::available_parallelism().map_or(0, |n| n.get());
    let brand = platform_brand().unwrap_or_else(|| std::env::consts::ARCH.to_string());
    format!("{brand} ({}, {threads} threads)", simd_level())
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn simd_level() -> &'static str {
    if is_x86_feature_detected!("avx512f") {
        "avx512f"
    } else if is_x86_feature_detected!("avx2") {
        "avx2"
    } else if is_x86_feature_detected!("fma") {
        "fma"
    } else {
        "sse2"
    }
}

#[cfg(target_arch = "aarch64")]
fn simd_level() -> &'static str {
    // NEON is part of the aarch64 baseline.
    "neon"
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
fn simd_level() -> &'static str {
    "unknown simd"
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn platform_brand() -> Option<String> {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::{__cpuid, CpuidResult};
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::{__cpuid, CpuidResult};

    // The brand string is stored in extended leaves 2 through 4.
    let highest_extended = __cpuid(0x8000_0000).eax;
    if highest_extended < 0x8000_0004 {
        return None;
    }

    let mut bytes = Vec::with_capacity(48);
    for leaf in 0x8000_0002..=0x8000_0004u32 {
        let CpuidResult { eax, ebx, ecx, edx } = __cpuid(leaf);
        for register in [eax, ebx, ecx, edx] {
            bytes.extend_from_slice(&register.to_le_bytes());
        }
    }

    // Normalize the padded 48-byte brand string.
    let text = String::from_utf8_lossy(&bytes);
    let brand = text.trim_matches('\0').split_whitespace().collect::<Vec<_>>().join(" ");
    (!brand.is_empty()).then_some(brand)
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn platform_brand() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::summary;

    #[test]
    fn reports_a_thread_count_and_an_instruction_set() {
        let summary = summary();
        assert!(summary.contains("threads"), "{summary}");
        assert!(!summary.starts_with('('), "{summary}");
    }

    #[test]
    fn is_stable_across_calls() {
        assert_eq!(summary(), summary());
    }
}
