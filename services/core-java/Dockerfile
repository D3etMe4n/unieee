FROM docker.io/library/eclipse-temurin:25-jdk-alpine AS builder
WORKDIR /workspace

COPY gradlew settings.gradle.kts build.gradle.kts lombok.config ./
COPY gradle gradle
RUN chmod +x gradlew && ./gradlew --no-daemon dependencies || true

COPY src src
RUN ./gradlew --no-daemon bootJar -x test \
    && cp "$(find /workspace/build/libs -maxdepth 1 -name '*.jar' ! -name '*-plain.jar' | head -n 1)" /workspace/app.jar

FROM docker.io/library/eclipse-temurin:25-jre-alpine AS runtime
WORKDIR /app

RUN addgroup -S unieee && adduser -S -G unieee unieee
COPY --from=builder /workspace/app.jar /app/app.jar
RUN chown unieee:unieee /app/app.jar

ENV JAVA_OPTS="-XX:+UseCompactObjectHeaders"

USER unieee
EXPOSE 8080

ENTRYPOINT ["sh", "-c", "exec java $JAVA_OPTS -jar /app/app.jar"]
