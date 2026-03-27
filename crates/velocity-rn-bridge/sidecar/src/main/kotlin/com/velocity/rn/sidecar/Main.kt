package com.velocity.rn.sidecar

import java.net.ServerSocket

fun main(args: Array<String>) {
    val port = args.firstOrNull()?.toIntOrNull() ?: 19500
    println("[velocity-rn-sidecar] Starting on port $port")

    val server = BridgeServer(port)
    server.start()
}
