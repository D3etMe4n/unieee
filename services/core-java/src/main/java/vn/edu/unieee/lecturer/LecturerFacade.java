package vn.edu.unieee.lecturer;

import java.util.List;
import java.util.UUID;
import vn.edu.unieee.lecturer.dto.CreateLecturerRequest;
import vn.edu.unieee.lecturer.dto.LecturerResponse;
import vn.edu.unieee.lecturer.dto.ResetLecturerPasswordRequest;
import vn.edu.unieee.lecturer.dto.UpdateLecturerRequest;
import vn.edu.unieee.lecturer.dto.UpdateLecturerStatusRequest;

public interface LecturerFacade {

    LecturerResponse createLecturer(CreateLecturerRequest request);

    LecturerResponse getLecturerById(UUID lecturerId);

    LecturerResponse getLecturerByKeycloakSub(String keycloakSub);

    List<LecturerResponse> listLecturers();

    LecturerResponse updateLecturer(UUID lecturerId, UpdateLecturerRequest request);

    LecturerResponse updateLecturerStatus(UUID lecturerId, UpdateLecturerStatusRequest request);

    void resetLecturerPassword(UUID lecturerId, ResetLecturerPasswordRequest request);

    boolean existsLecturer(UUID lecturerId);
}
