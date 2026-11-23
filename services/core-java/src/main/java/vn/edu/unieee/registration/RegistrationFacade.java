package vn.edu.unieee.registration;

import java.util.List;
import java.util.UUID;
import vn.edu.unieee.registration.dto.AdminEnrollmentAddRequest;
import vn.edu.unieee.registration.dto.AdminEnrollmentDropRequest;
import vn.edu.unieee.registration.dto.AdminEnrollmentTransferRequest;
import vn.edu.unieee.registration.dto.EnrollmentEventResponse;
import vn.edu.unieee.registration.dto.EnrollmentResponse;
import vn.edu.unieee.registration.dto.ReconciliationReportResponse;
import vn.edu.unieee.registration.dto.SectionRosterEntryResponse;

public interface RegistrationFacade {

    EnrollmentResponse adminAdd(AdminEnrollmentAddRequest request);

    EnrollmentResponse adminDrop(AdminEnrollmentDropRequest request);

    EnrollmentResponse adminTransfer(AdminEnrollmentTransferRequest request);

    List<EnrollmentResponse> getStudentTimetable(UUID termId);

    List<SectionRosterEntryResponse> getSectionRoster(Long sectionId);

    List<EnrollmentEventResponse> listStudentEnrollmentEvents(UUID studentId);

    ReconciliationReportResponse reconcileTerm(UUID termId);
}
