import java.lang.IllegalArgumentException;
import org.codeberg.musicparser.Formats;
import org.testng.annotations.Test;

class ParserTests {

    Parser SUT = new Parser();
    
    @Test
    void parseValidInput() {
        String input = "fis g a b a b g c b c a d es d c";

        Melody melody = null;
        try {
            melody = SUT.parse(input, Formats.LILYPOND);
        } catch(IllegalArgumentException e) {
            Assert.fail();
        }

        Assert.assertEquals(15, melody.length());
    } 
}
