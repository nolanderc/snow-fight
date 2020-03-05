use rabbit_derive::*;

fn assert_lossless<T>(before: &T)
where
    T: rabbit::PackBits + rabbit::UnpackBits + PartialEq + std::fmt::Debug,
{
    let bytes = rabbit::to_bytes(before).unwrap();
    let after: T = rabbit::from_bytes(&bytes).unwrap();
    assert_eq!(before, &after);
}

#[test]
fn one_variant() {
    #[derive(Debug, PartialEq, PackBits, UnpackBits)]
    enum Foo {
        Bar,
    }

    assert_lossless(&Foo::Bar);
}

#[test]
fn two_variants() {
    #[derive(Debug, PartialEq, PackBits, UnpackBits)]
    enum Foo {
        Bar,
        Baz,
    }

    assert_lossless(&Foo::Bar);
    assert_lossless(&Foo::Baz);
}

#[test]
fn three_variants() {
    #[derive(Debug, PartialEq, PackBits, UnpackBits)]
    enum Foo {
        A,
        B,
        C,
    }

    assert_lossless(&Foo::A);
    assert_lossless(&Foo::B);
    assert_lossless(&Foo::C);
}

#[test]
fn data_variants() {
    #[derive(Debug, PartialEq, PackBits, UnpackBits)]
    enum Foo {
        A(u32),
        B(u32, u32),
        C { a: u32, b: u32 },
    }

    assert_lossless(&Foo::A(123));
    assert_lossless(&Foo::B(456, 789));
    assert_lossless(&Foo::C {
        a: 123456789,
        b: 4711,
    });
}
