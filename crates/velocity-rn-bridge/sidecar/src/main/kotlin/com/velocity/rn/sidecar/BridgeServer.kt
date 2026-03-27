package com.velocity.rn.sidecar

import com.google.gson.Gson
import com.google.gson.JsonObject
import java.io.BufferedReader
import java.io.InputStreamReader
import java.io.PrintWriter
import java.net.ServerSocket
import java.net.Socket
import java.util.Base64

class BridgeServer(private val port: Int) {
    private val gson = Gson()
    private var renderer: HeadlessRenderer? = null
    private var running = true

    fun start() {
        val server = ServerSocket(port)
        println("[velocity-rn-sidecar] Listening on port $port")

        while (running) {
            val client = server.accept()
            println("[velocity-rn-sidecar] Client connected from ${client.inetAddress}")
            handleClient(client)
        }

        server.close()
    }

    private fun handleClient(socket: Socket) {
        val reader = BufferedReader(InputStreamReader(socket.getInputStream()))
        val writer = PrintWriter(socket.getOutputStream(), true)

        try {
            var line: String?
            while (reader.readLine().also { line = it } != null) {
                val response = handleCommand(line!!)
                writer.println(gson.toJson(response))
            }
        } catch (e: Exception) {
            println("[velocity-rn-sidecar] Client error: ${e.message}")
        } finally {
            socket.close()
        }
    }

    private fun handleCommand(json: String): Map<String, Any?> {
        return try {
            val cmd = gson.fromJson(json, JsonObject::class.java)
            val cmdType = cmd.get("cmd")?.asString ?: return errorResponse("Missing 'cmd' field")

            when (cmdType) {
                "init" -> handleInit(cmd)
                "get_hierarchy" -> handleGetHierarchy()
                "screenshot" -> handleScreenshot()
                "tap" -> handleTap(cmd)
                "input_text" -> handleInputText(cmd)
                "swipe" -> handleSwipe(cmd)
                "navigate" -> handleNavigate(cmd)
                "shutdown" -> {
                    running = false
                    okResponse(null)
                }
                else -> errorResponse("Unknown command: $cmdType")
            }
        } catch (e: Exception) {
            errorResponse("Command failed: ${e.message}")
        }
    }

    private fun handleInit(cmd: JsonObject): Map<String, Any?> {
        val bundlePath = cmd.get("bundle_path")?.asString ?: return errorResponse("Missing bundle_path")
        val component = cmd.get("component")?.asString ?: "App"
        val width = cmd.get("width")?.asInt ?: 1080
        val height = cmd.get("height")?.asInt ?: 2340

        renderer = HeadlessRenderer(bundlePath, component, width, height)
        renderer?.initialize()

        return okResponse(null)
    }

    private fun handleGetHierarchy(): Map<String, Any?> {
        val r = renderer ?: return errorResponse("Not initialized — send 'init' first")
        val hierarchy = r.getHierarchy()
        return okResponse(hierarchy)
    }

    private fun handleScreenshot(): Map<String, Any?> {
        val r = renderer ?: return errorResponse("Not initialized")
        val pngBytes = r.screenshot()
        val b64 = Base64.getEncoder().encodeToString(pngBytes)
        return okResponse(b64)
    }

    private fun handleTap(cmd: JsonObject): Map<String, Any?> {
        val r = renderer ?: return errorResponse("Not initialized")
        val x = cmd.get("x")?.asInt ?: 0
        val y = cmd.get("y")?.asInt ?: 0
        r.tap(x, y)
        return okResponse(null)
    }

    private fun handleInputText(cmd: JsonObject): Map<String, Any?> {
        val r = renderer ?: return errorResponse("Not initialized")
        val text = cmd.get("text")?.asString ?: ""
        r.inputText(text)
        return okResponse(null)
    }

    private fun handleSwipe(cmd: JsonObject): Map<String, Any?> {
        val r = renderer ?: return errorResponse("Not initialized")
        val fromX = cmd.get("from_x")?.asInt ?: 0
        val fromY = cmd.get("from_y")?.asInt ?: 0
        val toX = cmd.get("to_x")?.asInt ?: 0
        val toY = cmd.get("to_y")?.asInt ?: 0
        r.swipe(fromX, fromY, toX, toY)
        return okResponse(null)
    }

    private fun handleNavigate(cmd: JsonObject): Map<String, Any?> {
        val r = renderer ?: return errorResponse("Not initialized")
        val component = cmd.get("component")?.asString ?: return errorResponse("Missing component")
        r.navigate(component)
        return okResponse(null)
    }

    private fun okResponse(data: Any?): Map<String, Any?> {
        return mapOf("status" to "ok", "data" to data)
    }

    private fun errorResponse(message: String): Map<String, Any?> {
        return mapOf("status" to "error", "message" to message)
    }
}
