@org.springframework.modulith.ApplicationModule(
    allowedDependencies = {
        "common",
        "catalog",
        "catalog::dto",
        "catalog::event",
        "account",
        "account::dto",
        "account::event"
    }
)
package vn.edu.unieee.registration;
