#include<stdio.h>

class MyClass {
    void run(int argc) {
        printf("too few arguments\n");
        exit(1);
    }

    void run(bool reallyFast) {
        printf("world\n");
    }
}
