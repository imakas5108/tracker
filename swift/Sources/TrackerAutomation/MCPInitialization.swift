import Foundation

public enum MCPInitialization {
    public static func compatibleMessage(_ data: Data) -> Data {
        guard var message = try? JSONSerialization.jsonObject(with:data) as? [String:Any],
              message["method"] as? String == "initialize",
              var params = message["params"] as? [String:Any],
              var capabilities = params["capabilities"] as? [String:Any],
              let experimental = capabilities["experimental"] as? [String:Any] else { return data }
        // SDK 0.11.0 types experimental values as strings. Tracker implements none of
        // the object-valued extensions, so omit only those unsupported declarations.
        capabilities["experimental"] = experimental.filter { $0.value is String }
        params["capabilities"] = capabilities; message["params"] = params
        return (try? JSONSerialization.data(withJSONObject:message)) ?? data
    }
}
