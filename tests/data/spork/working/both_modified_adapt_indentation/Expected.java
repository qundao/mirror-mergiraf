class MyClass {
    boolean reallyEnabled = true;
    int myMethod() {
        boolean enabled = false;
        if (enabled) {
            onlyIfEnabled();
        }
    }

    void onlyIfEnabled() {
        if (reallyEnabled) {
            doSomething("param");
            checkThisOut("badoum", "tss");
            getBusy(123);
        }
    }
}
