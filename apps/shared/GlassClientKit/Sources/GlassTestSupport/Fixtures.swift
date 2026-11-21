import Foundation

/// Recorded gateway responses for tests in GlassClientKit and its apps.
public enum Fixtures {
    public enum Error: Swift.Error { case missing(String) }

    public static func data(_ name: String, ext: String = "json") throws -> Data {
        guard let url = Bundle.module.url(forResource: name, withExtension: ext, subdirectory: "Fixtures") else {
            throw Error.missing("\(name).\(ext)")
        }
        return try Data(contentsOf: url)
    }
}
