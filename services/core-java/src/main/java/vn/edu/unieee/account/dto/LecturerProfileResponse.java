package vn.edu.unieee.account.dto;

import java.util.UUID;

public record LecturerProfileResponse(
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
