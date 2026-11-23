package vn.edu.unieee.academic.internal.service;

import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Service;
import vn.edu.unieee.academic.AcademicFacade;
import vn.edu.unieee.academic.dto.AcademicTermResponse;
import vn.edu.unieee.academic.dto.AddCurriculumCourseRequest;
import vn.edu.unieee.academic.dto.CreateAcademicTermRequest;
import vn.edu.unieee.academic.dto.CreateCurriculumRequest;
import vn.edu.unieee.academic.dto.CreateFacultyRequest;
import vn.edu.unieee.academic.dto.CreateMajorRequest;
import vn.edu.unieee.academic.dto.CurriculumResponse;
import vn.edu.unieee.academic.dto.FacultyResponse;
import vn.edu.unieee.academic.dto.MajorResponse;
import vn.edu.unieee.academic.dto.TransitionAcademicTermStatusRequest;
import vn.edu.unieee.academic.internal.service.structure.AcademicStructureService;
import vn.edu.unieee.academic.internal.service.term.AcademicTermService;

@Service
@RequiredArgsConstructor
public class AcademicFacadeImpl implements AcademicFacade {

    private final AcademicTermService termService;
    private final AcademicStructureService structureService;

    @Override
    public AcademicTermResponse createTerm(CreateAcademicTermRequest request) {
        return termService.createTerm(request);
    }

    @Override
    public AcademicTermResponse getTermById(UUID termId) {
        return termService.getTermById(termId);
    }

    @Override
    public AcademicTermResponse getTermByCode(String code) {
        return termService.getTermByCode(code);
    }

    @Override
    public List<AcademicTermResponse> listTerms() {
        return termService.listTerms();
    }

    @Override
    public AcademicTermResponse transitionTermStatus(UUID termId, TransitionAcademicTermStatusRequest request) {
        return termService.transitionStatus(termId, request);
    }

    @Override
    public boolean existsTerm(UUID termId) {
        return termService.existsTerm(termId);
    }

    @Override
    public FacultyResponse createFaculty(CreateFacultyRequest request) {
        return structureService.createFaculty(request);
    }

    @Override
    public List<FacultyResponse> listFaculties() {
        return structureService.listFaculties();
    }

    @Override
    public boolean existsFacultyCode(String code) {
        return structureService.existsFacultyCode(code);
    }

    @Override
    public MajorResponse createMajor(CreateMajorRequest request) {
        return structureService.createMajor(request);
    }

    @Override
    public List<MajorResponse> listMajors() {
        return structureService.listMajors();
    }

    @Override
    public boolean existsMajorCode(String code) {
        return structureService.existsMajorCode(code);
    }

    @Override
    public CurriculumResponse createCurriculum(CreateCurriculumRequest request) {
        return structureService.createCurriculum(request);
    }

    @Override
    public CurriculumResponse addCourseToCurriculum(UUID curriculumId, AddCurriculumCourseRequest request) {
        return structureService.addCourseToCurriculum(curriculumId, request);
    }

    @Override
    public CurriculumResponse getCurriculumByMajorAndCohort(UUID majorId, String cohortCode) {
        return structureService.getCurriculumByMajorAndCohort(majorId, cohortCode);
    }
}
