// What counts as a subscription key in a file a person edits by hand.

import Testing

@testable import OTransitKit

@Suite("Key")
struct KeyTests {
    @Test("the key is read from the name either port writes")
    func names() {
        #expect(Key.find(in: "OC_TRANSPO_SUBSCRIPTION_KEY=abc123") == "abc123")
        #expect(Key.find(in: "SUBSCRIPTION_KEY=abc123") == "abc123")
        // A file copied from either of the other ports works unedited.
        #expect(Key.find(in: "OCT_SUBSCRIPTION_KEY=abc123") == "abc123")
    }

    @Test("comments, blank lines and other variables are skipped")
    func noise() {
        let file = """
            # Get this from the developer portal

            SOMETHING_ELSE=nope
            OC_TRANSPO_SUBSCRIPTION_KEY=abc123
            """
        #expect(Key.find(in: file) == "abc123")
    }

    @Test("quotes and spaces are the shell's, not the key's")
    func trimming() {
        #expect(Key.find(in: "OC_TRANSPO_SUBSCRIPTION_KEY = \"abc123\" ") == "abc123")
        #expect(Key.find(in: "export OC_TRANSPO_SUBSCRIPTION_KEY='abc123'") == "abc123")
    }

    @Test("an unedited template is no key at all")
    func placeholder() {
        // Better to say "scheduled times only" than to ask the endpoint about
        // the literal string your_key_here.
        #expect(Key.find(in: "OC_TRANSPO_SUBSCRIPTION_KEY=your_key_here") == nil)
        #expect(Key.find(in: "OC_TRANSPO_SUBSCRIPTION_KEY=") == nil)
        #expect(Key.find(in: "# nothing here") == nil)
    }

    @Test("a file saved with CRLF does not leave a carriage return on the key")
    func crlf() {
        // The value goes into an HTTP header, where a \r fails every request —
        // and a failing key looks exactly like an absent one from the board,
        // which reads `scheduled` either way. `.whitespaces` does not cover a
        // carriage return; `.whitespacesAndNewlines` does.
        #expect(Key.find(in: "OCT_SUBSCRIPTION_KEY=abc123\r\n") == "abc123")
        #expect(Key.find(in: "# a comment\r\nSUBSCRIPTION_KEY=\"abc123\"\r\n") == "abc123")
        #expect(Key.find(in: "export OC_TRANSPO_SUBSCRIPTION_KEY=abc123\r") == "abc123")
    }
}
