package vn.edu.unieee.lecturer.dto;

import java.util.UUID;

public record LecturerResponse(
    UUID id,
    String lecturerCode,
    String keycloakSub,
    String fullName,
    String email,
    String departmentCode,
    String academicTitle,
    String status
) {
}
