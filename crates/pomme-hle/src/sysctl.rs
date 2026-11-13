//! `sysctl` and `sysctlbyname`: the kernel's facts about the phone, as apps
//! and SDKs read them (the model for analytics, the CPU count for thread
//! pools, the boot time, and whether a debugger is attached, which crash
//! reporters check through `KERN_PROC_PID`).
//!
//! As on Darwin: a null buffer asks for the size; a buffer too small fails
//! with ENOMEM; names pomme doesn't know fail with ENOENT; setting values
//! fails with EPERM (an app can't).

use crate::{Emu, HostFn, Result};

// errno
const EPERM: u64 = 1;
const ENOENT: u64 = 2;
const ENOMEM: u64 = 12;

// MIB names: CTL_KERN and CTL_HW.
const CTL_KERN: u32 = 1;
const CTL_HW: u32 = 6;
const KERN_OSTYPE: u32 = 1;
const KERN_OSRELEASE: u32 = 2;
const KERN_VERSION: u32 = 4;
const KERN_HOSTNAME: u32 = 10;
const KERN_PROC: u32 = 14;
const KERN_PROC_PID: u32 = 1;
const KERN_BOOTTIME: u32 = 21;
const KERN_OSVERSION: u32 = 65;
const HW_MACHINE: u32 = 1;
const HW_NCPU: u32 = 3;
const HW_PAGESIZE: u32 = 7;
const HW_MEMSIZE: u32 = 24;
const HW_AVAILCPU: u32 = 25;

/// The phone's CPU cores and memory (an A18's six cores, 8 GB).
const CPUS: u32 = 6;
const MEMORY: u64 = 8 << 30;
const PAGE_SIZE: u32 = 16384;

/// `sizeof(struct kinfo_proc)` on arm64, and `kp_proc.p_flag`'s offset in it.
const KINFO_PROC_SIZE: usize = 648;
const P_FLAG: usize = 32;
/// `p_flag`: the process runs 64-bit code (P_LP64); not P_TRACED, as no debugger is attached.
const P_LP64: u32 = 0x4;

/// iOS releases' build numbers, for `kern.osversion`.
const BUILDS: &[((u32, u32), &str)] = &[((26, 0), "23A341"), ((18, 0), "22A3354"), ((17, 0), "21A329")];

/// A value, as its bytes.
fn value(emu: &Emu, name: &str) -> Option<Vec<u8>> {
    let cstr = |s: &str| {
        let mut v = s.as_bytes().to_vec();
        v.push(0);
        v
    };
    let os = emu.system.os;
    let release = format!("{}.0.0", os.darwin_major());
    Some(match name {
        "hw.machine" => cstr(emu.device.model()),
        "hw.ncpu" | "hw.activecpu" | "hw.availcpu" | "hw.logicalcpu" | "hw.physicalcpu" | "hw.logicalcpu_max" | "hw.physicalcpu_max" => CPUS.to_le_bytes().to_vec(),
        "hw.memsize" | "hw.physmem" => MEMORY.to_le_bytes().to_vec(),
        "hw.pagesize" => PAGE_SIZE.to_le_bytes().to_vec(),
        "kern.ostype" => cstr("Darwin"),
        "kern.osrelease" => cstr(&release),
        "kern.version" => cstr(&format!("Darwin Kernel Version {release}: pomme")),
        "kern.osversion" => cstr(BUILDS.iter().find(|((maj, min), _)| (*maj, *min) == (os.major(), os.minor()))?.1),
        "kern.hostname" => cstr("pomme"),
        "kern.boottime" => {
            // struct timeval: seconds and microseconds.
            let mut v = (crate::libsystem::EPOCH_BASE_SECS as i64).to_le_bytes().to_vec();
            v.extend_from_slice(&0i64.to_le_bytes());
            v
        }
        _ => return None,
    })
}

/// A MIB as its name, for the ones `value` answers.
fn mib_name(mib: &[u32]) -> Option<&'static str> {
    Some(match mib {
        [CTL_KERN, KERN_OSTYPE] => "kern.ostype",
        [CTL_KERN, KERN_OSRELEASE] => "kern.osrelease",
        [CTL_KERN, KERN_VERSION] => "kern.version",
        [CTL_KERN, KERN_HOSTNAME] => "kern.hostname",
        [CTL_KERN, KERN_BOOTTIME] => "kern.boottime",
        [CTL_KERN, KERN_OSVERSION] => "kern.osversion",
        [CTL_HW, HW_MACHINE] => "hw.machine",
        [CTL_HW, HW_NCPU] => "hw.ncpu",
        [CTL_HW, HW_PAGESIZE] => "hw.pagesize",
        [CTL_HW, HW_MEMSIZE] => "hw.memsize",
        [CTL_HW, HW_AVAILCPU] => "hw.availcpu",
        _ => return None,
    })
}

/// `struct kinfo_proc` for this process: zeros but for its flags.
fn kinfo_proc() -> Vec<u8> {
    let mut v = vec![0u8; KINFO_PROC_SIZE];
    v[P_FLAG..P_FLAG + 4].copy_from_slice(&P_LP64.to_le_bytes());
    v
}

fn fail(emu: &mut Emu, errno: u64) -> Result<()> {
    crate::libsystem::set_errno(emu, errno)?;
    emu.ret(u64::MAX);
    Ok(())
}

/// Copy `v` out as `sysctl` does: (oldp, oldlenp, newp).
fn answer(emu: &mut Emu, v: Option<Vec<u8>>, old: u64, oldlen: u64, new: u64) -> Result<()> {
    if new != 0 {
        return fail(emu, EPERM);
    }
    let Some(v) = v else { return fail(emu, ENOENT) };
    if old != 0 {
        let room = if oldlen == 0 { 0 } else { emu.read_u64(oldlen)? as usize };
        if room < v.len() {
            return fail(emu, ENOMEM);
        }
        emu.p.memory.write(old, &v)?;
    }
    if oldlen != 0 {
        emu.write_u64(oldlen, v.len() as u64)?;
    }
    emu.ret(0);
    Ok(())
}

/// `sysctlbyname(name, oldp, oldlenp, newp, newlen)`
fn sysctlbyname(emu: &mut Emu) -> Result<()> {
    let name = emu.cstr(emu.arg(0))?;
    let (old, oldlen, new) = (emu.arg(1), emu.arg(2), emu.arg(3));
    let v = value(emu, &name);
    answer(emu, v, old, oldlen, new)
}

/// `sysctl(mib, namelen, oldp, oldlenp, newp, newlen)`
fn sysctl(emu: &mut Emu) -> Result<()> {
    let (mib, n) = (emu.arg(0), emu.arg(1).min(12) as usize);
    let (old, oldlen, new) = (emu.arg(2), emu.arg(3), emu.arg(4));
    let mut names = Vec::with_capacity(n);
    for i in 0..n {
        let b = emu.p.memory.read(mib + i as u64 * 4, 4)?;
        names.push(u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    }
    let pid = crate::libsystem::PID as u32;
    let v = match names[..] {
        [CTL_KERN, KERN_PROC, KERN_PROC_PID, p] if p == pid => Some(kinfo_proc()),
        _ => mib_name(&names).and_then(|name| value(emu, name)),
    };
    answer(emu, v, old, oldlen, new)
}

/// `sysctlnametomib(name, mibp, sizep)`
fn sysctlnametomib(emu: &mut Emu) -> Result<()> {
    let name = emu.cstr(emu.arg(0))?;
    let (mibp, sizep) = (emu.arg(1), emu.arg(2));
    let all: [&[u32]; 11] = [
        &[CTL_KERN, KERN_OSTYPE],
        &[CTL_KERN, KERN_OSRELEASE],
        &[CTL_KERN, KERN_VERSION],
        &[CTL_KERN, KERN_HOSTNAME],
        &[CTL_KERN, KERN_BOOTTIME],
        &[CTL_KERN, KERN_OSVERSION],
        &[CTL_HW, HW_MACHINE],
        &[CTL_HW, HW_NCPU],
        &[CTL_HW, HW_PAGESIZE],
        &[CTL_HW, HW_MEMSIZE],
        &[CTL_HW, HW_AVAILCPU],
    ];
    let Some(mib) = all.into_iter().find(|m| mib_name(m) == Some(name.as_str())) else { return fail(emu, ENOENT) };
    let room = emu.read_u64(sizep)? as usize;
    if room < mib.len() {
        return fail(emu, ENOMEM);
    }
    for (i, v) in mib.iter().enumerate() {
        emu.p.memory.write(mibp + i as u64 * 4, &v.to_le_bytes())?;
    }
    emu.write_u64(sizep, mib.len() as u64)?;
    emu.ret(0);
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[("_sysctl", sysctl), ("_sysctlbyname", sysctlbyname), ("_sysctlnametomib", sysctlnametomib)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_mibs_agree() {
        assert_eq!(mib_name(&[CTL_HW, HW_MACHINE]), Some("hw.machine"));
        assert_eq!(mib_name(&[CTL_KERN, KERN_BOOTTIME]), Some("kern.boottime"));
        assert_eq!(mib_name(&[CTL_HW, 99]), None);
    }

    #[test]
    fn no_debugger() {
        let k = kinfo_proc();
        assert_eq!(k.len(), KINFO_PROC_SIZE);
        let flags = u32::from_le_bytes(k[P_FLAG..P_FLAG + 4].try_into().unwrap());
        assert_eq!(flags & 0x800, 0, "P_TRACED is clear");
    }
}
