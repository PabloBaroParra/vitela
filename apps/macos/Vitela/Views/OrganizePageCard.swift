import SwiftUI

/// One page in the Organize grid: its picture, its number and the actions
/// that apply to it. `onEdit` receives a page edit still missing its page
/// position, so the card never has to know which index it sits at.
struct OrganizePageCard: View {
    let number: Int
    let thumbnail: RenderedPage?
    let enabled: Bool
    /// False for a document's only page: a PDF needs at least one.
    let canDelete: Bool
    let isLast: Bool
    let onEdit: ((Int) -> PageEdit) -> Void

    var body: some View {
        VStack(spacing: 6) {
            picture
                .frame(width: ViewerStore.thumbnailBox, height: ViewerStore.thumbnailBox)
            HStack(spacing: 2) {
                Text("\(number)")
                    .font(.system(.body, design: .monospaced))
                Spacer()
                Button { onEdit { .rotate(page: $0, degrees: -90) } } label: {
                    Image(systemName: "rotate.left")
                }
                .help("Rotate page left")
                .accessibilityLabel("Rotate page \(number) left")
                Button { onEdit { .rotate(page: $0, degrees: 90) } } label: {
                    Image(systemName: "rotate.right")
                }
                .help("Rotate page right")
                .accessibilityLabel("Rotate page \(number) right")
                Button { onEdit { .remove(page: $0) } } label: {
                    Image(systemName: "trash")
                }
                .help("Delete page")
                .accessibilityLabel("Delete page \(number)")
                .disabled(!canDelete)
            }
            .buttonStyle(.borderless)
            .disabled(!enabled)
            Menu("Page actions") {
                Button("Move page earlier") { onEdit { .move(from: $0, to: $0 - 1) } }
                    .disabled(number == 1)
                Button("Move page later") { onEdit { .move(from: $0, to: $0 + 1) } }
                    .disabled(isLast)
                Divider()
                Button("Insert portrait A4 before") { onEdit { .insertBlank(at: $0, orientation: .portrait) } }
                Button("Insert landscape A4 before") { onEdit { .insertBlank(at: $0, orientation: .landscape) } }
            }
            .disabled(!enabled)
        }
        .padding(8)
        .frame(width: ViewerStore.thumbnailBox + 16)
        .background(RoundedRectangle(cornerRadius: 8).fill(Color(nsColor: .controlBackgroundColor)))
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Page \(number)")
    }

    @ViewBuilder private var picture: some View {
        if let image = thumbnail?.nsImage {
            Image(nsImage: image)
                .resizable()
                .aspectRatio(contentMode: .fit)
                .background(Color.white)
                .shadow(radius: 1)
        } else {
            ProgressView()
        }
    }
}
