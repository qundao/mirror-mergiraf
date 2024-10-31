class MyClass {
        void myMethod() {
                try {
                        doSomething();
                } catch (IOException | NullPointerException e) {
                        System.out.println("oops\n");
                }
        }
}
