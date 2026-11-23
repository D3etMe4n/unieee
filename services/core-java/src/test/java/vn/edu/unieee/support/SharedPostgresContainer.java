package vn.edu.unieee.support;

import org.testcontainers.postgresql.PostgreSQLContainer;
import org.testcontainers.utility.DockerImageName;

public final class SharedPostgresContainer {

    public static final PostgreSQLContainer INSTANCE = new PostgreSQLContainer(
        DockerImageName.parse("docker.io/library/postgres:17-alpine").asCompatibleSubstituteFor("postgres")
    );

    static {
        INSTANCE.start();
    }

    private SharedPostgresContainer() {
    }
}
