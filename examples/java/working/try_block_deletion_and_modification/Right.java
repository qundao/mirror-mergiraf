public class SeparatorBasedImporterTests extends ImporterTest {

    @Test(groups = {}, dataProvider = "CSV-TSV-AutoDetermine")
    public void readDoesNotTrimLeadingTrailingWhitespaceOnNoTrimStrings(String sep) {
        // create input to test with
        String inputSeparator = sep == null ? "\t" : sep;
        String input = " data1 " + inputSeparator + " 3.4 " + inputSeparator + " data3 ";

        prepareOptions(sep, -1, 0, 0, 0, false, false, false);
        parseOneFile(SUT, new StringReader(input));

        Project expectedProject = createProject(
                new String[] { numberedColumn(1), numberedColumn(2), numberedColumn(3) },
                new Serializable[][] {
                        { " data1 ", " 3.4 ", " data3 " },
                });
        assertProjectEquals(project, expectedProject);
    }
}
