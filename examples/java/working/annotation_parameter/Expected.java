class MyClass {
        @JsonProperty(value = "property", required = true, access = ACCESS.READ_ONLY)
        public int getValue() {
                return 23;
        }
}
