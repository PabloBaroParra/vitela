import SwiftUI

struct PageView: View {
    let slot: PageSlot
    let zoom: Double
    var onRetry: () -> Void = {}

    var body: some View {
        Group {
            if let image = slot.image, let nsImage = image.nsImage {
                Image(nsImage: nsImage)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .accessibilityLabel("Page \(slot.index + 1)")
            } else if case let .failed(message) = slot.status {
                failure(message)
            } else {
                ProgressView("Rendering page \(slot.index + 1)")
                    .frame(maxWidth: .infinity, minHeight: placeholderHeight)
            }
        }
        .frame(width: CGFloat(slot.dimensions.width * zoom))
        .background(Color.white)
        .shadow(radius: 1)
    }

    private func failure(_ message: String) -> some View {
        VStack(spacing: 8) {
            Text("Page \(slot.index + 1) could not render: \(message)")
                .multilineTextAlignment(.center)
            Button("Retry", action: onRetry)
        }
        .padding()
        .frame(maxWidth: .infinity, minHeight: placeholderHeight)
    }

    private var placeholderHeight: CGFloat {
        CGFloat(slot.dimensions.height * zoom)
    }
}
