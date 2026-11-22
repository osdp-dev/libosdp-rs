use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::copy("memory.x", out.join("memory.x")).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=memory.x");

    // LibOSDP formats its log messages and default PD names with snprintf()
    // and vsnprintf(); take them from newlib-nano in the Arm GNU toolchain,
    // using the multilib that matches thumbv7em-none-eabihf.
    let libc = Command::new("arm-none-eabi-gcc")
        .args([
            "-mthumb",
            "-mcpu=cortex-m4",
            "-mfloat-abi=hard",
            "-mfpu=fpv4-sp-d16",
            "-print-file-name=libc_nano.a",
        ])
        .output()
        .expect("arm-none-eabi-gcc is needed to locate newlib");
    let libc = PathBuf::from(String::from_utf8(libc.stdout).unwrap().trim());
    println!("cargo:rustc-link-search={}", libc.parent().unwrap().display());
    println!("cargo:rustc-link-lib=static=c_nano");
}
