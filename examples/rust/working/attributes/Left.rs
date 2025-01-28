mod tests {
    pub fn foo() {}

    #[cfg(feature = "tracing")]
    pub fn execute() {}
}
