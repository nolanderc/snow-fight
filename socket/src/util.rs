/// Source: https://doc.rust-lang.org/std/mem/union.MaybeUninit.html
macro_rules! arr {
    ($val:expr; $len:expr) => {{
        use ::std::mem::{self, MaybeUninit};

        // (this might panic... of consequence to nobody, anywhere)
        let x = $val;

        // Create an uninitialized array of `MaybeUninit`. The `assume_init` is
        // safe because the type we are claiming to have initialized here is a
        // bunch of `MaybeUninit`s, which do not require initialization.
        let mut data: [MaybeUninit<_>; $len] = unsafe {
            MaybeUninit::uninit().assume_init()
        };

        // Dropping a `MaybeUninit` does nothing. Thus using raw pointer
        // assignment instead of `ptr::write` does not cause the old
        // uninitialized value to be dropped. Also if there is a panic during
        // this loop, we have a memory leak, but there is no memory safety
        // issue.
        for elem in &mut data[..] {
            *elem = MaybeUninit::new(x.clone());
        }

        // Everything is initialized. Transmute the array to the
        // initialized type.
        unsafe { mem::transmute::<_, [_; $len]>(data) }
    }};
}
