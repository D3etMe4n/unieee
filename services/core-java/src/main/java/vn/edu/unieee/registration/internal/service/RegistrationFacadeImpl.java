package vn.edu.unieee.registration.internal.service;

import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Component;
import vn.edu.unieee.registration.RegistrationFacade;
import vn.edu.unieee.registration.dto.AdminEnrollmentAddRequest;
import vn.edu.unieee.registration.dto.AdminEnrollmentDropRequest;
import vn.edu.unieee.registration.dto.AdminEnrollmentTransferRequest;
import vn.edu.unieee.registration.dto.EnrollmentEventResponse;
import vn.edu.unieee.registration.dto.EnrollmentResponse;
import vn.edu.unieee.registration.dto.ReconciliationReportResponse;
import vn.edu.unieee.registration.dto.SectionRosterEntryResponse;
import vn.edu.unieee.registration.internal.service.override.AdminEnrollmentOverrideService;
import vn.edu.unieee.registration.internal.service.query.EnrollmentQueryService;

@Component
@RequiredArgsConstructor
public class RegistrationFacadeImpl implements RegistrationFacade {

    private final AdminEnrollmentOverrideService overrideService;
    private final EnrollmentQueryService queryService;

    @Override
    public EnrollmentResponse adminAdd(AdminEnrollmentAddRequest request) {
        return overrideService.adminAdd(request);
    }

    @Override
    public EnrollmentResponse adminDrop(AdminEnrollmentDropRequest request) {
        return overrideService.adminDrop(request);
    }

    @Override
    public EnrollmentResponse adminTransfer(AdminEnrollmentTransferRequest request) {
        return overrideService.adminTransfer(request);
    }

    @Override
    public List<EnrollmentResponse> getStudentTimetable(UUID termId) {
        return queryService.getStudentTimetable(termId);
    }

    @Override
    public List<SectionRosterEntryResponse> getSectionRoster(Long sectionId) {
        return queryService.getSectionRoster(sectionId);
    }

    @Override
    public List<EnrollmentEventResponse> listStudentEnrollmentEvents(UUID studentId) {
        return queryService.listStudentEnrollmentEvents(studentId);
    }

    @Override
    public ReconciliationReportResponse reconcileTerm(UUID termId) {
        return queryService.reconcileTerm(termId);
    }
}
