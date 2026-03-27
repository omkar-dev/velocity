plugins {
    kotlin("jvm") version "1.9.22"
    application
}

group = "com.velocity"
version = "0.1.0"

repositories {
    mavenCentral()
    google()
}

dependencies {
    // Android LayoutLib for headless rendering
    // Note: actual LayoutLib JARs need to be sourced from Android SDK
    // This is a scaffold — production use requires:
    //   implementation(files("libs/layoutlib.jar"))

    implementation("com.google.code.gson:gson:2.10.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.7.3")

    testImplementation(kotlin("test"))
}

application {
    mainClass.set("com.velocity.rn.sidecar.MainKt")
}

tasks.test {
    useJUnitPlatform()
}
