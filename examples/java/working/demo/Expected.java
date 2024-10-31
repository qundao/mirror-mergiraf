
import org.codeberg.musicparser.Formats;
import org.testng.annotations.Test;

class ParserTests {

    Parser SUT = new Parser();
    
    @Test
    void parseValidInput() throws Exception {
        String input = "fis g a b a b g c b c a d es d c";

        Melody melody = SUT.parse(input, Formats.LILYPOND);

        Assert.assertEquals(15, melody.length());
    } 
}
