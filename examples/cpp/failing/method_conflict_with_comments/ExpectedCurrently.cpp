#include<stdio.h>

class MyClass {
    void run(int argc) {
        printf("too few arguments\n");
        exit(1);
    }
<<<<<<< LEFT
    // central backbone of the algorithm
    void run() {
        printf("hello\n");
    }
||||||| BASE
=======
    void run(bool reallyFast) {
        printf("world\n");
    }
>>>>>>> RIGHT
}
