// Kotlin DSL
plugins {
    kotlin("jvm") version "2.0.0"
    application
}

val ktorVersion: String by project

repositories {
    mavenCentral()
}

dependencies {
    implementation("io.ktor:ktor-server-core:$ktorVersion")
    testImplementation(kotlin("test"))
}

application {
    mainClass.set("org.example.MainKt")
}

tasks.register("hello") {
    doLast { println("Hello ${project.name}") }
}

fun answer(x: Int): Int = x * 42
