package vn.edu.unieee.common.dto;

import java.util.List;

public record CurrentUserResponse(
    String subject,
    String username,
    String email,
    List<String> roles,
    String studentId,
    String lecturerId
) {
}
