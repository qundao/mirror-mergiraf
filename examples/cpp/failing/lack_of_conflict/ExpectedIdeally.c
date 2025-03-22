#include<stdio.h>

int main(int argc, char** argv) {
    if (argc < 2) {
        printf("too few arguments\n");
        exit(1);
    }
    if (argc == 3) {
        printf("hello\n");
    }
    if (argc == 4) {
        printf("world\n");
    }
}
