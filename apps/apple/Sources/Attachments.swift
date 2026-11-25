import PDFKit
import SwiftUI
import UniformTypeIdentifiers

struct AttachmentInfo: Identifiable, Hashable {
    var id: String { file }
    let file: String
    let label: String
    let kind: String
    let url: URL
}

extension LibraryModel {
    func attachments(_ songId: String) -> [AttachmentInfo] {
        guard let out = run("song.get", ["id": songId]), let r = out["result"] as? [String: Any] else { return [] }
        return (r["attachments"] as? [[String: Any]] ?? []).compactMap { a in
            guard let file = a["file"] as? String, let path = a["path"] as? String else { return nil }
            return AttachmentInfo(file: file, label: (a["label"] as? String) ?? file, kind: a["kind"] as? String ?? "other",
                                  url: URL(fileURLWithPath: path))
        }
    }

    /// Copy a picked file into the song (the URL may be security-scoped).
    func attach(_ songId: String, url: URL) {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        run("song.attach", ["id": songId, "path": url.path(percentEncoded: false)])
        bump()
    }

    func detach(_ songId: String, file: String) {
        run("song.detach", ["id": songId, "file": file])
        bump()
    }
}

/// A PDF shown with PDFKit (fit to width, continuous scrolling, dark-mode friendly).
struct PDFKitView: UIViewRepresentable {
    let url: URL
    var invert = false

    func makeUIView(context: Context) -> PDFView {
        let v = PDFView()
        v.autoScales = true
        v.displayMode = .singlePageContinuous
        v.displayDirection = .vertical
        v.document = PDFDocument(url: url)
        return v
    }

    func updateUIView(_ v: PDFView, context: Context) {
        if v.document?.documentURL != url { v.document = PDFDocument(url: url) }
        v.backgroundColor = invert ? .black : .secondarySystemBackground
    }
}

/// Buttons for a song's attachments plus "attach".
struct AttachmentBar: View {
    @Environment(LibraryModel.self) private var model
    let songId: String
    @Binding var showing: AttachmentInfo?
    @State private var importing = false

    var body: some View {
        let items = model.attachments(songId)
        ScrollView(.horizontal, showsIndicators: false) {
            HStack {
                ForEach(items) { a in
                    Button {
                        showing = (showing == a) ? nil : a
                    } label: {
                        Label(a.label, systemImage: icon(a.kind))
                    }
                    .buttonStyle(.bordered)
                    .tint(showing == a ? .accentColor : .secondary)
                    .contextMenu {
                        Button("Remove attachment", role: .destructive) { model.detach(songId, file: a.file) }
                    }
                }
                Button { importing = true } label: { Label("Attach", systemImage: "paperclip") }
                    .buttonStyle(.borderless)
            }
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.pdf, .image, .audio, .midi, .plainText]) { result in
            if case .success(let url) = result { model.attach(songId, url: url) }
        }
    }

    private func icon(_ kind: String) -> String {
        switch kind {
        case "pdf": return "doc.richtext"
        case "image": return "photo"
        case "audio": return "waveform"
        case "midi": return "pianokeys"
        default: return "doc"
        }
    }
}
