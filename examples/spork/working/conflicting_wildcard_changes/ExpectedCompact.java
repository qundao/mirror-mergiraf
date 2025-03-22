import java.util.List;

class Cls {
    public static void print(
<<<<<<< LEFT
List<? super String>
||||||| BASE
List
=======
List<? extends String>
>>>>>>> RIGHT
 list) {
        System.out.println(list);
    }
}
