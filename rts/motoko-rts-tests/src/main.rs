mod bigint;
mod bitmap;
mod bitrel;
mod continuation_table;
mod crc32;
mod gc;
mod leb128;
mod memory;
mod principal_id;
mod stream;
mod text;
mod utf8;

use motoko_rts::types::Bytes;

fn main() {
    if std::mem::size_of::<usize>() != 4 {
        println!("Motoko RTS only works on 32-bit architectures");
        std::process::exit(1);
    }

    unsafe {
        bigint::test();
        bitmap::test();
        // bitrel::test();
        continuation_table::test();
        crc32::test();
        gc::test();
        leb128::test();
        principal_id::test();
        stream::test();
        text::test();
        utf8::test();
    }
}

// Called by the RTS to panic
#[no_mangle]
extern "C" fn rts_trap(ptr: *const u8, len: Bytes<u32>) -> ! {
    let msg = unsafe { std::slice::from_raw_parts(ptr, len.as_usize()) };
    match core::str::from_utf8(msg) {
        Err(err) => panic!(
            "rts_trap_with called with non-UTF8 string (error={:?}, string={:?})",
            err, msg
        ),
        Ok(str) => panic!("rts_trap_with: {:?}", str),
    }
}

// Called by RTS BigInt functions to panic. Normally generated by the compiler
#[no_mangle]
extern "C" fn bigint_trap() -> ! {
    panic!("bigint_trap called");
}

// Called by the RTS for debug prints
#[no_mangle]
unsafe extern "C" fn print_ptr(ptr: usize, len: u32) {
    let str: &[u8] = core::slice::from_raw_parts(ptr as *const u8, len as usize);
    println!("[RTS] {}", String::from_utf8_lossy(str));
}
