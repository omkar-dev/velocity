package com.velocity.sync

import android.util.Log
import org.json.JSONArray
import org.json.JSONObject
import java.io.BufferedReader
import java.io.InputStreamReader
import java.io.PrintWriter
import java.net.ServerSocket
import java.net.Socket
import kotlin.concurrent.thread

/**
 * Lightweight HTTP-like server that exposes app idle state.
 * Listens on localhost:19401 (accessible via adb port-forward).
 *
 * Protocol: newline-delimited JSON over TCP (same as iOS probe).
 *
 * Commands:
 *   {"cmd": "status"}
 *   {"cmd": "wait_idle", "timeout_ms": 5000}
 */
class SyncServer(
    private val stateTracker: AppStateTracker,
    private val port: Int
) {
    private var serverSocket: ServerSocket? = null
    @Volatile private var running = false

    fun start() {
        running = true
        thread(name = "velocity-sync-server", isDaemon = true) {
            try {
                serverSocket = ServerSocket(port).also {
                    it.reuseAddress = true
                }
                Log.i(TAG, "Sync server listening on port $port")

                while (running) {
                    try {
                        val client = serverSocket?.accept() ?: break
                        handleClient(client)
                    } catch (e: Exception) {
                        if (running) {
                            Log.w(TAG, "Accept error: ${e.message}")
                        }
                    }
                }
            } catch (e: Exception) {
                Log.e(TAG, "Server failed to start: ${e.message}")
            }
        }
    }

    fun stop() {
        running = false
        try { serverSocket?.close() } catch (_: Exception) {}
        serverSocket = null
    }

    private fun handleClient(socket: Socket) {
        thread(name = "velocity-sync-client", isDaemon = true) {
            try {
                val reader = BufferedReader(InputStreamReader(socket.getInputStream()))
                val writer = PrintWriter(socket.getOutputStream(), true)

                while (running) {
                    val line = reader.readLine() ?: break
                    if (line.isBlank()) continue

                    val response = processCommand(line)
                    writer.println(response)
                }
            } catch (e: Exception) {
                Log.d(TAG, "Client disconnected: ${e.message}")
            } finally {
                try { socket.close() } catch (_: Exception) {}
            }
        }
    }

    private fun processCommand(json: String): String {
        return try {
            val command = JSONObject(json)
            when (command.optString("cmd")) {
                "status" -> statusResponse()
                "wait_idle" -> {
                    val timeoutMs = command.optLong("timeout_ms", 5000)
                    waitIdleResponse(timeoutMs)
                }
                else -> """{"error":"unknown command"}"""
            }
        } catch (e: Exception) {
            """{"error":"${e.message}"}"""
        }
    }

    private fun statusResponse(): String {
        val idle = stateTracker.isIdle
        val pending = JSONArray(stateTracker.pendingReasons)
        val ts = System.currentTimeMillis() / 1000
        return JSONObject().apply {
            put("idle", idle)
            put("pending", pending)
            put("ts", ts)
        }.toString()
    }

    private fun waitIdleResponse(timeoutMs: Long): String {
        val start = System.currentTimeMillis()
        val deadline = start + timeoutMs

        while (System.currentTimeMillis() < deadline) {
            if (stateTracker.isIdle) {
                val elapsed = System.currentTimeMillis() - start
                return JSONObject().apply {
                    put("idle", true)
                    put("pending", JSONArray())
                    put("waited_ms", elapsed)
                }.toString()
            }
            Thread.sleep(5) // 5ms poll interval
        }

        val pending = JSONArray(stateTracker.pendingReasons)
        return JSONObject().apply {
            put("idle", false)
            put("pending", pending)
            put("waited_ms", timeoutMs)
        }.toString()
    }

    companion object {
        private const val TAG = "VelocitySyncProbe"
    }
}
