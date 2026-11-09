//! Compile-time contracts of the option derives.

#[cfg(test)]
mod tests {
    #[test]
    fn the_derives_refuse_attributes_they_would_ignore() {
        let cases = trybuild::TestCases::new();
        cases.pass("tests/ui/skip_default.rs");
        cases.compile_fail("tests/ui/skip_with_key.rs");
        cases.compile_fail("tests/ui/skip_and_flatten.rs");
        cases.compile_fail("tests/ui/limits_on_text.rs");
        cases.compile_fail("tests/ui/limits_out_of_order.rs");
        cases.compile_fail("tests/ui/inline_with_key.rs");
        cases.compile_fail("tests/ui/inline_on_leaf.rs");
        cases.compile_fail("tests/ui/variant_attribute.rs");
        cases.compile_fail("tests/ui/default_at_type.rs");
    }
}
