package vn.edu.unieee.academic.internal.service.structure;

import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.academic.dto.AddCurriculumCourseRequest;
import vn.edu.unieee.academic.dto.CreateCurriculumRequest;
import vn.edu.unieee.academic.dto.CreateFacultyRequest;
import vn.edu.unieee.academic.dto.CreateMajorRequest;
import vn.edu.unieee.academic.dto.CurriculumResponse;
import vn.edu.unieee.academic.dto.FacultyResponse;
import vn.edu.unieee.academic.dto.MajorResponse;
import vn.edu.unieee.academic.internal.domain.Curriculum;
import vn.edu.unieee.academic.internal.domain.Faculty;
import vn.edu.unieee.academic.internal.domain.Major;
import vn.edu.unieee.academic.internal.exception.AcademicConflictException;
import vn.edu.unieee.academic.internal.exception.AcademicResourceNotFoundException;
import vn.edu.unieee.academic.internal.mapper.AcademicMapper;
import vn.edu.unieee.academic.internal.repository.CurriculumRepository;
import vn.edu.unieee.academic.internal.repository.FacultyRepository;
import vn.edu.unieee.academic.internal.repository.MajorRepository;

@Service
@RequiredArgsConstructor
public class AcademicStructureService {

    private final FacultyRepository facultyRepository;
    private final MajorRepository majorRepository;
    private final CurriculumRepository curriculumRepository;
    private final AcademicMapper mapper;

    @Transactional
    public FacultyResponse createFaculty(CreateFacultyRequest request) {
        String code = request.code().trim();
        if (facultyRepository.existsByCode(code)) {
            throw new AcademicConflictException(
                "FACULTY_ALREADY_EXISTS",
                "Faculty already exists with code: " + code
            );
        }
        Faculty saved = facultyRepository.save(Faculty.create(code, request.name()));
        return mapper.toFacultyResponse(saved);
    }

    @Transactional(readOnly = true)
    public List<FacultyResponse> listFaculties() {
        return facultyRepository.findAllByOrderByCodeAsc().stream()
            .map(mapper::toFacultyResponse)
            .toList();
    }

    @Transactional(readOnly = true)
    public boolean existsFacultyCode(String code) {
        return code != null && facultyRepository.existsByCode(code.trim());
    }

    @Transactional
    public MajorResponse createMajor(CreateMajorRequest request) {
        String code = request.code().trim();
        if (majorRepository.existsByCode(code)) {
            throw new AcademicConflictException(
                "MAJOR_ALREADY_EXISTS",
                "Major already exists with code: " + code
            );
        }
        Faculty faculty = facultyRepository.findById(request.facultyId())
            .orElseThrow(() -> new AcademicResourceNotFoundException(
                "FACULTY_NOT_FOUND",
                "Faculty not found: " + request.facultyId()
            ));
        Major saved = majorRepository.save(Major.create(faculty, code, request.name(), request.totalRequiredCredits()));
        return mapper.toMajorResponse(saved);
    }

    @Transactional(readOnly = true)
    public List<MajorResponse> listMajors() {
        return majorRepository.findAllByOrderByCodeAsc().stream()
            .map(mapper::toMajorResponse)
            .toList();
    }

    @Transactional(readOnly = true)
    public boolean existsMajorCode(String code) {
        return code != null && majorRepository.existsByCode(code.trim());
    }

    @Transactional
    public CurriculumResponse createCurriculum(CreateCurriculumRequest request) {
        Major major = majorRepository.findWithFacultyById(request.majorId())
            .orElseThrow(() -> new AcademicResourceNotFoundException(
                "MAJOR_NOT_FOUND",
                "Major not found: " + request.majorId()
            ));
        String cohortCode = request.cohortCode().trim();
        if (curriculumRepository.existsByMajorIdAndCohortCode(major.getId(), cohortCode)) {
            throw new AcademicConflictException(
                "CURRICULUM_ALREADY_EXISTS",
                "Curriculum already exists for major " + major.getCode() + " and cohort " + cohortCode
            );
        }
        Curriculum saved = curriculumRepository.save(Curriculum.create(major, cohortCode, request.name()));
        return mapper.toCurriculumResponse(saved);
    }

    @Transactional
    public CurriculumResponse addCourseToCurriculum(UUID curriculumId, AddCurriculumCourseRequest request) {
        Curriculum curriculum = curriculumRepository.findWithDetailsById(curriculumId)
            .orElseThrow(() -> new AcademicResourceNotFoundException(
                "CURRICULUM_NOT_FOUND",
                "Curriculum not found: " + curriculumId
            ));
        curriculum.addCourse(request.courseCode(), request.recommendedSemester(), request.mandatory());
        Curriculum saved = curriculumRepository.saveAndFlush(curriculum);
        return mapper.toCurriculumResponse(saved);
    }

    @Transactional(readOnly = true)
    public CurriculumResponse getCurriculumByMajorAndCohort(UUID majorId, String cohortCode) {
        Curriculum curriculum = curriculumRepository.findByMajorIdAndCohortCode(majorId, cohortCode.trim())
            .orElseThrow(() -> new AcademicResourceNotFoundException(
                "CURRICULUM_NOT_FOUND",
                "Curriculum not found for major " + majorId + " and cohort " + cohortCode
            ));
        return mapper.toCurriculumResponse(curriculum);
    }
}
