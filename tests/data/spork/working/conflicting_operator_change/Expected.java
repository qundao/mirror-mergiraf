public class Adder {
    public int add(int a, int b) {
<<<<<<< LEFT
        return a * b;
||||||| BASE
        return a + b;
=======
        return a / b;
>>>>>>> RIGHT
    }

    public int otherOps() {
        int a = 1;
<<<<<<< LEFT
        a -= 1;
||||||| BASE
        a += 1;
=======
        a *= 1;
>>>>>>> RIGHT

<<<<<<< LEFT
        a = +a;
||||||| BASE
        a = ~a;
=======
        a = -a;
>>>>>>> RIGHT

        return a;
    }
}
