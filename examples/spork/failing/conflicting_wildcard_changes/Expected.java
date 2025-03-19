import java.util.List;

class Cls {
<<<<<<< LEFT
    public static void print(List<? super String> list) {
||||||| BASE
    public static void print(List list) {
=======
    public static void print(List<? extends String> list) {
>>>>>>> RIGHT
        System.out.println(list);
    }
}
