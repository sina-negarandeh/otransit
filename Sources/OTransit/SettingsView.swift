// Settings: the subscription key today, and whatever is added beside it later.
//
// One screen, reached from the bottom of the first one and left by the same
// back button every other screen has. It is not a `Place`: a place in this app
// is an answer about a journey, and this is not one.
//
// The key is the only setting here, and it is the one that changes what the
// program can do rather than how it looks. Without it every time on screen is a
// scheduled one — a working program, said plainly here rather than left to be
// inferred from a board where no bus is ever late.
//
// Each section is its own View and not a computed property on this one. They
// look alike and are not: a computed property is inlined into the enclosing
// body, so it shares that body's invalidation boundary and every keystroke in
// the field would redraw the status, the buttons and the footer with it.

import OTransitKit
import SwiftUI

struct SettingsView: View {
    let back: () -> Void
    /// Text to start the key field with, for a screen asked for by name.
    var sample: String?

    var body: some View {
        VStack(spacing: 0) {
            TopBar(screen: .settings, back: back)
            Rule()
            KeySettings(sample: sample)
        }
    }
}

/// Everything about the subscription key: whether there is one, the field that
/// replaces it, and where to get one.
private struct KeySettings: View {
    /// What is typed, which is not what is stored. The stored key is never read
    /// back into the field: a key is write-only from here, because a row of
    /// dots standing in for a value nobody can check is worse than a sentence
    /// saying one is saved.
    @State private var typed: String
    @State private var showing: Bool
    @State private var note: String?

    /// Absent in a preview, which draws this screen without a running app. The
    /// key lives here and not on the disk as far as this screen is concerned:
    /// one owner, written through, read by everything else.
    @Environment(Schedule.self) private var schedule: Schedule?

    /// `sample` is text to start the field with, showing. It is how the
    /// revealed state gets photographed, and it is the driver's to supply — a
    /// view has no business knowing what a demonstration key looks like.
    init(sample: String? = nil) {
        _typed = State(initialValue: sample ?? "")
        _showing = State(initialValue: sample != nil)
    }

    private var stored: Bool { schedule?.key != nil }

    var body: some View {
        Form {
            Section {
                KeyState(on: stored)
                KeyField(typed: $typed, showing: $showing, stored: stored)
                KeyActions(typed: typed, stored: stored, save: save, remove: remove)
            } footer: {
                KeyFooter(note: note)
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
    }

    private func save() {
        guard Key.plausible(typed) else { return }
        do {
            try schedule?.use(typed)
            typed = ""
            showing = false
            note = "Saved. Live times appear on the next board you open."
        } catch {
            note = "That could not be saved: \(error)"
        }
    }

    private func remove() {
        schedule?.forget()
        typed = ""
        note = "Removed. Every time on screen is a scheduled one."
    }
}

/// What the key does, in the two states it can be in.
///
/// The state is the heading rather than a line under one, because it is the
/// answer to the only question this screen exists to settle.
private struct KeyState: View {
    let on: Bool

    var body: some View {
        LabeledContent {
            Text(on ? "On" : "Off")
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(on ? Color.green : .secondary)
        } label: {
            Label {
                Text("Live arrival times")
            } icon: {
                Image(systemName: on ? "dot.radiowaves.up.forward" : "clock")
                    .foregroundStyle(on ? Color.green : .secondary)
            }
        }
    }
}

private struct KeyField: View {
    @Binding var typed: String
    @Binding var showing: Bool
    let stored: Bool

    /// Whether the field had the caret when the eye was clicked. Clicking it
    /// while typing must not take the caret away; clicking it while not typing
    /// must not hand it over.
    @FocusState private var focused: Bool

    var body: some View {
        LabeledContent {
            HStack(spacing: 6) {
                // Concealed by default and revealable. A key pasted into a
                // field that cannot be read back is a key that cannot be
                // checked for the stray character that makes every request fail
                // silently.
                //
                // The placeholder goes in `prompt` and not in the title: a
                // title here is the label down the left of the row, where a
                // whole sentence set in the field's monospace wraps to two
                // lines and reads as the name of the setting.
                Group {
                    if showing {
                        TextField("Key", text: $typed, prompt: Text(prompt))
                    } else {
                        SecureField("Key", text: $typed, prompt: Text(prompt))
                    }
                }
                .labelsHidden()
                .focused($focused)
                .textFieldStyle(.roundedBorder)
                .font(.mono(.caption))
                // A form's trailing content is trailing-aligned, which puts a
                // key's first character against the right edge and fills
                // leftwards as it is typed. A key is read from its start.
                .multilineTextAlignment(.leading)

                Button {
                    // The caret, put back where it was.
                    //
                    // A TextField and a SecureField are two different views, so
                    // revealing a key replaces the one being typed into rather
                    // than changing it. The replacement arrives without focus,
                    // and clicking the eye half way through a key would drop
                    // the caret and send the rest of the typing nowhere.
                    let typing = focused
                    showing.toggle()
                    if typing {
                        // Next turn, not this one: the field that takes the
                        // focus does not exist until this change is applied.
                        Task { focused = true }
                    }
                } label: {
                    // An open eye offers to show it; a struck one offers to
                    // hide it. The symbol is what clicking will do, not what
                    // the field is doing now.
                    Image(systemName: showing ? "eye.slash" : "eye")
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                        .frame(width: 18)
                        .contentTransition(.symbolEffect(.replace))
                }
                .buttonStyle(.plain)
                .help(showing ? "Hide the key" : "Show the key")
            }
        } label: {
            Label {
                Text("Key")
            } icon: {
                Image(systemName: "key")
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var prompt: String { stored ? "Replace it" : "Paste a key" }
}

/// The two things that can be done, at the trailing edge.
///
/// Trailing because that is where a Mac puts the buttons that act on what is
/// above them, and the one that runs on Return is the last of them.
private struct KeyActions: View {
    let typed: String
    let stored: Bool
    let save: () -> Void
    let remove: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Spacer(minLength: 0)
            if stored {
                Button("Remove", role: .destructive, action: remove)
            }
            Button("Save", action: save)
                .buttonStyle(.borderedProminent)
                .disabled(!Key.plausible(typed))
                .keyboardShortcut(.defaultAction)
        }
    }
}

/// What just happened, and where a key comes from.
///
/// A screen that asks for a key and does not say where to get one is a dead end
/// for anyone who has not already been told.
private struct KeyFooter: View {
    let note: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            if let note {
                Text(note).foregroundStyle(.primary)
            }
            Text("A key is free from OC Transpo's developer portal.")
            Link(
                "nextrip-public-api.developer.azure-api.net",
                destination: URL(string: "https://nextrip-public-api.developer.azure-api.net")!)
        }
        .font(.system(size: 10.5))
        .fixedSize(horizontal: false, vertical: true)
        .padding(.top, 2)
    }
}
