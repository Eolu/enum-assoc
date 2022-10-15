use enum_assoc::Assoc;

// A bit of mock data
const WA: &'static str = "wa";
fn some_str_func(s: &'static str) -> String
{
    String::from("I was created in a function") + s
}

#[derive(Assoc)]
#[func(const fn foo(&self) -> u8)]
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

#[derive(Assoc)]
#[func(pub fn foo(&self, param: u8) -> Option<u8>)]
#[func(pub fn bar(&self, param: &'static str) -> String)]
#[func(pub fn baz<T: std::fmt::Debug>(&self, param: T) -> Option<String>)]
enum TestEnum2
{
    #[assoc(bar = String::new() + param)] 
    Variant1,
    #[assoc(foo = 12 + param)] 
    #[assoc(bar = String::from("Hello") + param)] 
    Variant2,
    #[assoc(bar = some_str_func("!"))] 
    #[assoc(baz = format!("{:?}", param))] 
    Variant3
}

// Including a module to test visibility identifiers
mod some_mod
{
    use enum_assoc::Assoc;

    #[derive(Assoc, Debug, PartialEq, Eq)]
    #[func(pub fn foo(s: &str) -> Option<Self>)]
    #[func(pub(crate) const fn bar(u: u8) -> Self)]
    #[func(pub fn baz(u1: u8, u2: u8) -> Self)]
    pub enum TestEnum3
    {
        #[assoc(foo = "variant1")] 
        #[assoc(bar = _)] 
        Variant1,
        #[assoc(bar = 2)] 
        #[assoc(foo = "variant2")] 
        #[assoc(baz = (3, 7))] 
        Variant2,
        #[assoc(foo = "I'm variant 3!")] 
        #[assoc(foo = "variant3")] 
        #[assoc(baz = _)] 
        Variant3
    }
}

#[derive(Assoc)]
#[func(pub fn foo(&'a self) -> Option<()>)]
pub enum TestEnum4<'a> 
{
    #[assoc(foo = ())] 
    Variant1 { some_str: &'a str },
    Variant2
}

#[derive(Assoc)]
#[func(pub fn foo(&'a self) -> Option<()>)]
pub enum TestEnum5<'a, 'b> 
{
    #[assoc(foo = ())] 
    Variant1 { some_str: &'a str, some_str_2: &'b str },
    Variant2
}

#[derive(Assoc)]
#[func(pub fn foo(&self, t: T) -> Option<u8>)]
pub enum TestEnum6<'a, T> 
{
    #[assoc(foo = 1)] 
    Variant1 { some_str: &'a str },
    Variant2,
    #[assoc(foo = 3)] 
    Variant3(T)
}

#[derive(Assoc)]
#[func(pub fn foo(&self) -> u8 { 0 } )]
pub enum TestEnumWithDefault
{
    #[assoc(foo = 1)]
    ValueSet,
    UsingDefault,
}


#[test]
fn test_fwd()
{
    assert_eq!(TestEnum::Variant1.foo(), 255);
    assert_eq!(TestEnum::Variant2.foo(), 8);
    assert_eq!(TestEnum::Variant3.foo(), 0);
    assert_eq!(TestEnum::Variant1.bar(), "wow");
    assert_eq!(TestEnum::Variant2.bar(), "wee");
    assert_eq!(TestEnum::Variant3.bar(), "wa");
    assert_eq!(TestEnum::Variant1.maybe_foo(), None);
    assert_eq!(TestEnum::Variant2.maybe_foo(), None);
    assert_eq!(TestEnum::Variant3.maybe_foo(), Some(20));
    assert_eq!(TestEnum2::Variant1.foo(0), None);
    assert_eq!(TestEnum2::Variant2.foo(22), Some(34));
    assert_eq!(TestEnum2::Variant1.bar("string"), "string");
    assert_eq!(TestEnum2::Variant2.bar(" World!"), "Hello World!");
    assert_eq!(TestEnum2::Variant3.bar("!"), "I was created in a function!");
    assert_eq!(TestEnum2::Variant3.baz(1), Some("1".to_string()));
}

#[test]
fn test_rev()
{
    use some_mod::TestEnum3;
    assert_eq!(TestEnum3::foo("variant1"), Some(TestEnum3::Variant1));
    assert_eq!(TestEnum3::foo("variant3"), Some(TestEnum3::Variant3));
    assert_eq!(TestEnum3::foo("I'm variant 3!"), Some(TestEnum3::Variant3));
    assert_eq!(TestEnum3::foo("I don't exist"), None);
    assert_eq!(TestEnum3::bar(2), TestEnum3::Variant2);
    assert_eq!(TestEnum3::bar(55), TestEnum3::Variant1);
    assert_eq!(TestEnum3::baz(3, 7), TestEnum3::Variant2);
    assert_eq!(TestEnum3::baz(0, 0), TestEnum3::Variant3);
}

#[test]
fn test_generics()
{
    assert_eq!(TestEnum4::Variant1{some_str: "wow"}.foo(), Some(()));
    assert_eq!(TestEnum4::Variant2.foo(), None);
    assert_eq!(TestEnum6::Variant1{some_str: "wow"}.foo(3), Some(1));
    assert_eq!(TestEnum6::Variant2.foo("this could be anything"), None);
    assert_eq!(TestEnum6::Variant3("macaroni").foo("cheese"), Some(3));
}



#[test]
fn test_default()
{
    assert_eq!(TestEnumWithDefault::ValueSet.foo(), 1);
    assert_eq!(TestEnumWithDefault::UsingDefault.foo(), 0);
}
