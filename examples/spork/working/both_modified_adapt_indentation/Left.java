class MyClass {
    boolean reallyEnabled = true;
    int myMethod() {
        boolean enabled = false;
        if (enabled) {
            onlyIfEnabled();
        }
        checkThisOut("badoum", "tss");
    }

    void onlyIfEnabled() {
        if (reallyEnabled) {
            doSomething("param");
            getBusy(123);
        }
    }
}
