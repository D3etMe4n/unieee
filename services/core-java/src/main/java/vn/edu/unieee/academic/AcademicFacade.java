package vn.edu.unieee.academic;

import java.util.List;
import java.util.UUID;
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

public interface AcademicFacade {

    AcademicTermResponse createTerm(CreateAcademicTermRequest request);

    AcademicTermResponse getTermById(UUID termId);

    AcademicTermResponse getTermByCode(String code);

    List<AcademicTermResponse> listTerms();

    AcademicTermResponse transitionTermStatus(UUID termId, TransitionAcademicTermStatusRequest request);

    boolean existsTerm(UUID termId);

    FacultyResponse createFaculty(CreateFacultyRequest request);

    List<FacultyResponse> listFaculties();

    boolean existsFacultyCode(String code);

    MajorResponse createMajor(CreateMajorRequest request);

    List<MajorResponse> listMajors();

    boolean existsMajorCode(String code);

    CurriculumResponse createCurriculum(CreateCurriculumRequest request);

    CurriculumResponse addCourseToCurriculum(UUID curriculumId, AddCurriculumCourseRequest request);

    CurriculumResponse getCurriculumByMajorAndCohort(UUID majorId, String cohortCode);
}
