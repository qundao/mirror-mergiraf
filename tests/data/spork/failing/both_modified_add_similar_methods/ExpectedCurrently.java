public class Arithmetic {

<<<<<<< LEFT
    public int div(int a, int b) {
        return a / b;
    }
||||||| BASE
=======
    public int div(int a, int b) {
        if (b == 0) {
            throw new IllegalArgumentException("b must be non-zero");
        }
        return a / b;
    }
>>>>>>> RIGHT
    public int add(int a, int b) {
    return a + b;
}

    public int sub(int a, int b) {
        return a - b;
    }
}
