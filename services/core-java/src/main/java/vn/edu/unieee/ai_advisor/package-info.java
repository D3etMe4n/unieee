@org.springframework.modulith.ApplicationModule(
    allowedDependencies = {
        "common",
        "catalog",
        "catalog::dto",
        "account",
        "account::dto",
        "registration",
        "registration::dto"
    }
)
package vn.edu.unieee.ai_advisor;
