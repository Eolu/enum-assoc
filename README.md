# enum-assoc

This crate defines a few macros that allow you to associate constants with enum variants. 

To use, `#[derive(Assoc)]` must be attached to an enum. From there, the `func` attribute is used to define function signtatures which will be attached to that enum. The `assoc` atribute is used to define constants which each variant will return when that function is called.

Here's an example:

```rust
use enum_assoc::Assoc;

const WA: &'static str = "wa";

#[derive(Assoc)]
#[func(pub fn foo(&self) -> u8)]
#[func(pub fn bar(&self) -> &'static str)]
#[func(pub fn maybe_foo(&self) -> Option<u8>)]
enum TestEnum
{
    #[assoc(foo = 255)] 
    #[assoc(bar = "wow")] 
    Variant1,
    #[assoc(foo = 1 + 7)] 
    #[assoc(bar = "wee")] 
    Variant2,
    #[assoc(foo = 0)]
    #[assoc(bar = WA)] 
    #[assoc(maybe_foo = 18 + 2)] 
    Variant3
}

fn main() 
{
    let _ = TestEnum::Variant1;
    println!("Variant1 foo: {}", TestEnum::Variant1.foo());
    println!("Variant2 foo: {}", TestEnum::Variant2.foo());
    println!("Variant3 foo: {}", TestEnum::Variant3.foo());
    println!("Variant1 s: {}", TestEnum::Variant1.bar());
    println!("Variant2 s: {}", TestEnum::Variant2.bar());
    println!("Variant3 s: {}", TestEnum::Variant3.bar());
    println!("Variant1 maybe_foo: {:?}", TestEnum::Variant1.maybe_foo());
    println!("Variant2 maybe_foo: {:?}", TestEnum::Variant2.maybe_foo());
    println!("Variant3 maybe_foo: {:?}", TestEnum::Variant3.maybe_foo());
}

```

Note that functions which return an `Option` type have special functionality: Variants may leave out the `assoc` attribute entirely to automatically return `None`, and variants which do yield a value need not explicitly wrap it in `Some`. 