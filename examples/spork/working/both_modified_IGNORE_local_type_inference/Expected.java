import java.util.Arrays;
import java.util.stream.IntStream;

class Cls {
    public static int sum(int... values) {
        var stream = Arrays.stream(values);
        return stream.reduce((var a, var b) -> a + b);
    }
}
