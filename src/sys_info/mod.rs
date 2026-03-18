// This tells the compiler: "If the target OS is linux, compile the linux.rs file"
#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "linux")]
pub use linux::*; // Re-export the Linux functions so the main app can use them easily