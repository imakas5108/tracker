import XCTest
import Darwin
@testable import TrackerAutomation

final class ProtocolTests: XCTestCase {
    func testPrivateRuntimeDirectoriesRejectBroadPermissionsAndSymlinks() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at:root,withIntermediateDirectories:false,attributes:[.posixPermissions:0o700])
        defer { try? FileManager.default.removeItem(at:root) }
        let runtime = root.appendingPathComponent("runtime")
        try LocalEndpoint.prepare(directory:runtime)
        var info = stat(); XCTAssertEqual(lstat(runtime.path,&info),0); XCTAssertEqual(info.st_mode & 0o777,0o700)
        XCTAssertEqual(chmod(runtime.path,0o755),0)
        XCTAssertThrowsError(try LocalEndpoint.prepare(directory:runtime))
        XCTAssertEqual(chmod(runtime.path,0o700),0)
        let link = root.appendingPathComponent("link")
        try FileManager.default.createSymbolicLink(at:link,withDestinationURL:runtime)
        XCTAssertThrowsError(try LocalEndpoint.prepare(directory:link))
    }
    func testRationalTimestampIntegerDoesNotPassThroughDouble() throws {
        struct Stamp: Codable { var value: Int64; var timescale: Int32 }
        let stamp = Stamp(value:9_123_499_531_892_099,timescale:1_000_000_000)
        let encoded = try JSONValue.from(stamp)
        XCTAssertEqual(try encoded.decode(Stamp.self).value,stamp.value)
        XCTAssertTrue(try encoded.text().contains("9123499531892099"))
    }
    func testCodexExperimentalCapabilityCompatibilityIsNarrow() throws {
        let input = Data(#"{"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{"experimental":{"codex/auth-change":{},"string-extension":"v1"},"elicitation":{"form":{},"url":{}}},"clientInfo":{"name":"codex","version":"0.166"}}}"#.utf8)
        let value = try JSONDecoder().decode(JSONValue.self,from:MCPInitialization.compatibleMessage(input))
        XCTAssertEqual(value["params"]["capabilities"]["experimental"].object?.count,1)
        XCTAssertEqual(value["params"]["capabilities"]["experimental"]["string-extension"].string,"v1")
        XCTAssertNotNil(value["params"]["capabilities"]["elicitation"]["url"].object)
        let other = Data(#"{"method":"tools/call","params":{"experimental":{"object":{}}}}"#.utf8)
        XCTAssertEqual(MCPInitialization.compatibleMessage(other),other)
    }
    func testWireRoundTripAndSchemas() throws {
        let request = AutomationRequest(command:"tracker_list_sessions",clientID:UUID())
        XCTAssertEqual(try JSONDecoder().decode(AutomationRequest.self,from:JSONEncoder().encode(request)).command,request.command)
        XCTAssertEqual(Set(ToolCatalog.tools.map(\.name)).count,25)
        _ = try ToolCatalog.validate(request)
        XCTAssertThrowsError(try ToolCatalog.validate(AutomationRequest(command:"tracker_seek",arguments:["session_id":.string("a")],clientID:UUID())))
    }
    func testAtomicNoClobber() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at:dir,withIntermediateDirectories:true)
        defer { try? FileManager.default.removeItem(at:dir) }
        let file = dir.appendingPathComponent("result")
        try AtomicOutput.write(Data("first".utf8),to:file,overwrite:false)
        XCTAssertThrowsError(try AtomicOutput.write(Data("second".utf8),to:file,overwrite:false))
        XCTAssertEqual(try String(contentsOf:file,encoding:.utf8),"first")
        try AtomicOutput.write(Data("second".utf8),to:file,overwrite:true)
        XCTAssertEqual(try String(contentsOf:file,encoding:.utf8),"second")
    }
    func testLocalFramingPeerAndExclusiveEndpoint() throws {
        let directory = URL(fileURLWithPath:"/private/tmp/tmcp-"+UUID().uuidString.prefix(8))
        try FileManager.default.createDirectory(at:directory,withIntermediateDirectories:false,attributes:[.posixPermissions:0o700])
        defer { try? FileManager.default.removeItem(at:directory) }
        let path = directory.appendingPathComponent("control.sock").path
        let server = LocalSocketServer(path:path)
        try server.start { request in AutomationResponse(result:.object(["command":.string(request.command)])) }
        defer { server.stop() }
        let second = LocalSocketServer(path:path)
        XCTAssertThrowsError(try second.start { _ in AutomationResponse(result:.null) })
        let connection = try LocalConnection.connect(path:path)
        let response = try connection.call(AutomationRequest(command:"hello",clientID:UUID()))
        XCTAssertEqual(response.result?["command"].string,"hello")
        var info = stat(); XCTAssertEqual(lstat(path,&info),0); XCTAssertEqual(info.st_mode & 0o777,0o600)
        server.stop()
        XCTAssertThrowsError(try LocalConnection.connect(path:path))
    }
    func testOversizedAndEmptyFramesAreRejected() throws {
        var descriptors: [Int32] = [0,0]
        XCTAssertEqual(socketpair(AF_UNIX,SOCK_STREAM,0,&descriptors),0)
        let writer = LocalConnection(fd:descriptors[0]), reader = LocalConnection(fd:descriptors[1])
        try writer.write(Data())
        XCTAssertThrowsError(try reader.read())
        try writer.write(Data(repeating:1,count:32))
        XCTAssertThrowsError(try reader.read(limit:16))
    }
}
