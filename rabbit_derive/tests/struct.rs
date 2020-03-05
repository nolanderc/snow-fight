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
fn unit() {
    #[derive(Debug, PartialEq, PackBits, UnpackBits)]
    struct Foo;

    assert_lossless(&Foo);
}

#[test]
fn single_field_tuple() {
    #[derive(Debug, PartialEq, PackBits, UnpackBits)]
    struct Foo(u32);

    assert_lossless(&Foo(32));
}

#[test]
fn two_fields_tuple() {
    #[derive(Debug, PartialEq, PackBits, UnpackBits)]
    struct Foo(u32, u32);

    assert_lossless(&Foo(123, 456));
}

#[test]
fn single_field_named() {
    #[derive(Debug, PartialEq, PackBits, UnpackBits)]
    struct Foo {
        bar: u32,
    }

    assert_lossless(&Foo { bar: 42 });
}

#[test]
fn two_fields_named() {
    #[derive(Debug, PartialEq, PackBits, UnpackBits)]
    struct Foo {
        bar: u32,
        baz: u32,
    }

    assert_lossless(&Foo {
        bar: 42,
        baz: 12839081,
    });
}

#[test]
fn nested_struct() {
    #[derive(Debug, PartialEq, PackBits, UnpackBits)]
    struct Outer {
        a: u32,
        inner: Inner,
    }

    #[derive(Debug, PartialEq, PackBits, UnpackBits)]
    struct Inner {
        b: u32,
        c: u32,
    }

    assert_lossless(&Outer {
        a: 0,
        inner: Inner { b: 1, c: 2 },
    });
}
