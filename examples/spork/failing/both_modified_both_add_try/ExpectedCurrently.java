class Cls {
    public static void main(String[] args) {
        try {
            System.out.println("Hello");
<<<<<<< LEFT
        } catch (IllegalArgumentException e) {
            System.out.println("Woopsie!");
            System.out.println("My bad!");
        } finally {
||||||| BASE
        }  finally {
=======
        } catch (IllegalArgumentException e) {
            System.out.println("Oopsie!");
            System.out.println("My bad!");
        } finally {
>>>>>>> RIGHT
            System.out.println("Bye bye!");
        }
    }
}
