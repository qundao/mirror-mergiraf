/**
 * This parameter add is technically conflicting, but should be automatically resolved with an optimistic conflict
 * handler.
 */
public class Adder {
<<<<<<< LEFT
    public int add(int a, int b, int c, int d, int e) {
||||||| BASE
    public int add(int a, int b) {
=======
    public int add(int a, int b, int c) {
>>>>>>> RIGHT
        return a + b;
    }
}
