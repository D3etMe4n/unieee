package vn.edu.unieee.common.security;

import java.util.List;

public record CurrentUser(
    String subject,
    String username,
    String email,
    List<String> roles,
    String studentId,
    String lecturerId
) {
}
