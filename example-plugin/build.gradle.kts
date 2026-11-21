plugins {
    `java-library`
}

group = "com.example"
version = "1.0.0"

repositories {
    mavenCentral()
    maven("https://repo.papermc.io/repository/maven-public/")
}

dependencies {
    compileOnly("io.papermc.paper:paper-api:${property("paperVersion")}")
}

java {
    toolchain {
        languageVersion.set(JavaLanguageVersion.of(21))
    }
}
