import Foundation
import Network

/// TCP server that exposes app idle state to the Velocity test runner.
/// Protocol: newline-delimited JSON over TCP on localhost.
///
/// Commands:
///   {"cmd": "status"}
///   {"cmd": "wait_idle", "timeout_ms": 5000}
///
/// Responses:
///   {"idle": true, "pending": [], "ts": 1710000000}
///   {"idle": false, "pending": ["animation:2", "network:1"], "waited_ms": 5000}
@available(iOS 12.0, *)
final class SyncServer {
    private let stateTracker: AppStateTracker
    private let port: UInt16
    private var listener: NWListener?

    init(stateTracker: AppStateTracker, port: UInt16) {
        self.stateTracker = stateTracker
        self.port = port
    }

    func start() {
        guard let nwPort = NWEndpoint.Port(rawValue: port) else { return }

        let params = NWParameters.tcp
        params.allowLocalEndpointReuse = true

        do {
            listener = try NWListener(using: params, on: nwPort)
        } catch {
            print("[VelocitySyncProbe] Failed to create listener: \(error)")
            return
        }

        listener?.newConnectionHandler = { [weak self] connection in
            self?.handleConnection(connection)
        }

        listener?.stateUpdateHandler = { state in
            switch state {
            case .ready:
                print("[VelocitySyncProbe] Sync server listening on port \(port)")
            case .failed(let error):
                print("[VelocitySyncProbe] Listener failed: \(error)")
            default:
                break
            }
        }

        listener?.start(queue: DispatchQueue(label: "com.velocity.sync-server"))
    }

    func stop() {
        listener?.cancel()
        listener = nil
    }

    private func handleConnection(_ connection: NWConnection) {
        connection.start(queue: DispatchQueue(label: "com.velocity.sync-client"))
        readLine(from: connection)
    }

    private func readLine(from connection: NWConnection) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 4096) { [weak self] data, _, isComplete, error in
            guard let self, let data, error == nil else {
                connection.cancel()
                return
            }

            if let line = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines),
               !line.isEmpty {
                let response = self.processCommand(line)
                let responseData = (response + "\n").data(using: .utf8) ?? Data()
                connection.send(content: responseData, completion: .contentProcessed { _ in
                    if !isComplete {
                        self.readLine(from: connection)
                    }
                })
            } else if !isComplete {
                self.readLine(from: connection)
            }
        }
    }

    private func processCommand(_ json: String) -> String {
        guard let data = json.data(using: .utf8),
              let command = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let cmd = command["cmd"] as? String
        else {
            return """
            {"error": "invalid command"}
            """
        }

        switch cmd {
        case "status":
            return statusResponse()

        case "wait_idle":
            let timeoutMs = command["timeout_ms"] as? Int ?? 5000
            return waitIdleResponse(timeoutMs: timeoutMs)

        default:
            return """
            {"error": "unknown command: \(cmd)"}
            """
        }
    }

    private func statusResponse() -> String {
        let idle = stateTracker.isIdle
        let pending = stateTracker.pendingReasons
        let ts = Int(Date().timeIntervalSince1970)
        let pendingJson = pending.map { "\"\($0)\"" }.joined(separator: ",")
        return """
        {"idle":\(idle),"pending":[\(pendingJson)],"ts":\(ts)}
        """
    }

    private func waitIdleResponse(timeoutMs: Int) -> String {
        let start = DispatchTime.now()
        let deadline = start + .milliseconds(timeoutMs)
        let pollInterval: UInt32 = 5_000 // 5ms in microseconds

        while DispatchTime.now() < deadline {
            if stateTracker.isIdle {
                let elapsed = Int((DispatchTime.now().uptimeNanoseconds - start.uptimeNanoseconds) / 1_000_000)
                return """
                {"idle":true,"pending":[],"waited_ms":\(elapsed)}
                """
            }
            usleep(pollInterval)
        }

        let pending = stateTracker.pendingReasons
        let pendingJson = pending.map { "\"\($0)\"" }.joined(separator: ",")
        return """
        {"idle":false,"pending":[\(pendingJson)],"waited_ms":\(timeoutMs)}
        """
    }
}
