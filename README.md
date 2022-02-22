# enum-assoc

This crate defines a few macros that allow you to associate constants or data with enum variants. 

To use, `#[derive(Assoc)]` must be attached to an enum. From there, the `func` attribute is used to define function signatures which will be implemented for that enum. The `assoc` attribute is used to define constants which each variant will return when that function is called.

## Forward associations

Here's an example:

```rust
use enum_assoc::Assoc;

const WA: &'static str = "wa";

#[derive(Assoc)]
#[func(pub const fn foo(&self) -> u8)]
#[func(pub fn bar(&self) -> &'static str)]
#[func(pub fn maybe_foo(&self) -> Option<u8>)]
enum TestEnum {
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

fn main() {
    println!("Variant1 foo: {}", TestEnum::Variant1.foo());
    println!("Variant2 foo: {}", TestEnum::Variant2.foo());
    println!("Variant3 foo: {}", TestEnum::Variant3.foo());
    println!("Variant1 bar: {}", TestEnum::Variant1.bar());
    println!("Variant2 bar: {}", TestEnum::Variant2.bar());
    println!("Variant3 bar: {}", TestEnum::Variant3.bar());
    println!("Variant1 maybe_foo: {:?}", TestEnum::Variant1.maybe_foo());
    println!("Variant2 maybe_foo: {:?}", TestEnum::Variant2.maybe_foo());
    println!("Variant3 maybe_foo: {:?}", TestEnum::Variant3.maybe_foo());
}

```
Output:
```ignore
Variant1 foo: 255
Variant2 foo: 8
Variant3 foo: 0
Variant1 bar: wow
Variant2 bar: wee
Variant3 bar: wa
Variant1 maybe_foo: None
Variant2 maybe_foo: None
Variant3 maybe_foo: Some(20)
```

Note that functions which return an `Option` type have special functionality: Variants may leave out the `assoc` attribute entirely to automatically return `None`, and variants which do yield a value need not explicitly wrap it in `Some`. 

### What does this output?

Every `#[func(fn_signature)]` attribute generates something like the following:

```rust,ignore
impl Enum {
    fn_signature {
        match self {
            // ... arms
        }
    }
}
```

And every `#[assoc(fn_name = association)]` attribute generates an arm for its associated function like the following:

```rust,ignore
    variant_name => association,
```

That's it. Both the details of the `fn_signature` you use and what you put in the `association` area are up to you.

So while technically not the original intention of this crate, you can generate some more interesting/complex associations for free:
```rust
use enum_assoc::Assoc;

#[derive(Assoc)]
#[func(pub fn foo(&self, param: u8) -> Option<u8>)]
#[func(pub fn bar(&self, param: &str) -> String)]
#[func(pub fn baz<T: std::fmt::Debug>(&self, param: T) -> Option<String>)]
enum TestEnum2 {
    #[assoc(bar = String::new() + param)] 
    Variant1,
    #[assoc(foo = 16 + param)] 
    #[assoc(bar = String::from("Hello") + param)] 
    Variant2,
    #[assoc(bar = some_str_func(param))] 
    #[assoc(baz = format!("{:?}", param))] 
    Variant3
}

fn some_str_func(s: &str) -> String {
    String::from("I was created in a function") + s
}

fn main() {
    println!("Variant1 foo: {:?}", TestEnum2::Variant1.foo(0));
    println!("Variant2 foo: {:?}", TestEnum2::Variant2.foo(22));
    println!("Variant1 bar: {}", TestEnum2::Variant1.bar("string"));
    println!("Variant2 bar: {}", TestEnum2::Variant2.bar(" World!"));
    println!("Variant3 bar: {}", TestEnum2::Variant3.bar("!"));
    println!("Variant3 baz: {:?}", TestEnum2::Variant3.baz(1));
}
```
Output:
```ignore
Variant1 foo: None
Variant2 foo: 34
Variant1 bar: string
Variant2 bar: Hello World!
Variant3 bar: I was created in a function!
Variant3 baz: Some("1")
```

## Reverse associations

This can also generate reverse associations (constants to enum variants). See below for an example.

```rust
use enum_assoc::Assoc;

#[derive(Assoc, Debug)]
#[func(pub fn reverse_foo(s: &str) -> Option<Self>)]
#[func(pub fn reverse_bar(u: u8) -> Self)]
#[func(pub fn reverse_baz(u1: u8, u2: u8) -> Self)]
enum TestEnum3
{
    #[assoc(reverse_foo = "variant1")] 
    #[assoc(reverse_bar = _)] 
    Variant1,
    #[assoc(reverse_bar = 2)] 
    #[assoc(reverse_foo = "variant2")] 
    #[assoc(reverse_baz = (3, 7))] 
    Variant2,
    #[assoc(reverse_foo = "I'm variant 3!")] 
    #[assoc(reverse_foo = "variant3")] 
    #[assoc(reverse_baz = _)] 
    Variant3
}

fn main() 
{
    println!("TestEnum3 reverse_foo(\"variant1\"): {:?}", TestEnum3::reverse_foo("variant1"));
    println!("TestEnum3 reverse_foo(\"variant3\"): {:?}", TestEnum3::reverse_foo("variant3"));
    println!("TestEnum3 reverse_foo(\"I'm variant 3!\"): {:?}", TestEnum3::reverse_foo("I'm variant 3!"));
    println!("TestEnum3 reverse_foo(\"I don't exist\"): {:?}", TestEnum3::reverse_foo("I don't exist"));
    println!("TestEnum3 reverse_bar(2): {:?}", TestEnum3::reverse_bar(2));
    println!("TestEnum3 reverse_bar(55): {:?}", TestEnum3::reverse_bar(55));
    println!("TestEnum3 reverse_baz(3, 7): {:?}", TestEnum3::reverse_baz(3, 7));
    println!("TestEnum3 reverse_baz(0, 0): {:?}", TestEnum3::reverse_baz(0, 0));
}

```

Reverse associations work slightly differently than forward associations: 
- Reverse associations must not include a `self` parameter (the lack of a `self` paramater is currently what differentiates a forward association from a reverse association)
- They must return either `Self` or `Option<Self>`
- Unlike forward associations, any number of `assoc` attributes for the same function may be defined for a single enum variant.
- Unlike forward associations, the `assoc` attribute defines a pattern rather than an expression. 
- The function generated will match on a tuple containing all of the function arguments. 
- The only guarantee regarding order of the match arms is that any wildcard pattern `_` will always be placed at the bottom.  
- If no wildcard pattern is defined for a function that returns `Option<Self>`, a `_ => None` arm will be inserted automatically.

Currently, there is no way for constants to map to tuple or struct-like variants.  

### What does this output?

Every `#[func(fn_signature)]` attribute for reverse associations generates something like the following:

```rust,ignore
impl Enum {
    fn_signature {
        match (param1, param2, etc) {
            // ... arms
        }
    }
}
```

And every `#[assoc(fn_name = pattern)]` attribute for reverse associations generates an arm for its associated function like the following:

```rust,ignore
    pattern => variant_name,
```