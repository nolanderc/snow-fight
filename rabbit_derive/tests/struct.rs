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

#[test]
fn custom_packing_fn() {
    #[derive(Debug, PartialEq, PackBits, UnpackBits)]
    struct Line {
        #[rabbit(pack = "point::pack", unpack = "point::unpack")]
        start: point::Point,
        #[rabbit(with = "point")]
        end: point::Point,
    }

    assert_lossless(&Line {
        start: point::Point { x: 0, y: 4 },
        end: point::Point { x: 37, y: 1 },
    });
}

mod point {
    use rabbit::{PackBits, ReadBits, UnpackBits, WriteBits};

    #[derive(Debug, PartialEq)]
    pub struct Point {
        pub x: u8,
        pub y: u8,
    }

    pub fn pack<W: WriteBits>(point: &Point, writer: &mut W) -> Result<(), W::Error> {
        point.x.pack(writer)?;
        point.y.pack(writer)?;
        Ok(())
    }

    pub fn unpack<R: ReadBits>(reader: &mut R) -> Result<Point, R::Error> {
        Ok(Point {
            x: u8::unpack(reader)?,
            y: u8::unpack(reader)?,
        })
    }
}
