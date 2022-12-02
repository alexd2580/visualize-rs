macro_rules! unsafe_unwrap {
    ($unsafe_expr:expr) => {
        unsafe { $unsafe_expr }.
        println!("Hello!");
    };
}
