use diagweave::set;

set! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    SetA = {
        Variant1,
    }

    #[derive(Clone, Debug, PartialEq)]
    SetB = SetA | {
        Variant2(String),
    }

    #[derive(Clone)]
    SetC = {
        Variant3,
    }
}

#[test]
fn test_set_derives_clone_copy() {
    let a = SetA::Variant1;
    let b = a; // Copy works
    assert_eq!(a, b); // PartialEq works
}

#[test]
fn test_set_derives_clone_only() {
    let b = SetB::Variant2("hello".to_string());
    let b2 = b.clone(); // Clone works
    assert_eq!(b, b2); // PartialEq works
}

#[test]
fn test_set_derives_implicit_debug() {
    let c = SetC::Variant3;
    let dbg = format!("{:?}", c); // Debug works (it's always added)
    assert!(dbg.contains("Variant3"));
}

#[test]
fn test_conversion_between_differently_derived_sets() {
    let a = SetA::Variant1;
    let b: SetB = a.into(); // Conversion works
    match b {
        SetB::Variant1 => {}
        _ => panic!("unexpected"),
    }
}
