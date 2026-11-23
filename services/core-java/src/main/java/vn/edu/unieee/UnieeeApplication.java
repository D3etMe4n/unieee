package vn.edu.unieee;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;
import org.springframework.modulith.Modulithic;
import org.springframework.scheduling.annotation.EnableScheduling;

@SpringBootApplication
@EnableScheduling
@Modulithic(sharedModules = "common")
public class UnieeeApplication {

    public static void main(String[] args) {
        SpringApplication.run(UnieeeApplication.class, args);
    }
}
