#include<stdio.h>

class MyClass {
    void run(int argc) {
        printf("too few arguments\n");
        exit(1);
    }
    // central backbone of the algorithm
    void run() {
        printf("hello\n");
    }
    void run(bool reallyFast) {
        printf("world\n");
    }
}
