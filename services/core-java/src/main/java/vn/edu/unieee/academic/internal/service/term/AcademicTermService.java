package vn.edu.unieee.academic.internal.service.term;

import java.time.Instant;
import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.context.ApplicationEventPublisher;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.academic.dto.AcademicTermResponse;
import vn.edu.unieee.academic.dto.CreateAcademicTermRequest;
import vn.edu.unieee.academic.dto.TransitionAcademicTermStatusRequest;
import vn.edu.unieee.academic.event.AcademicTermStatusChangedEvent;
import vn.edu.unieee.academic.internal.domain.AcademicTerm;
import vn.edu.unieee.academic.internal.domain.AcademicTermStatus;
import vn.edu.unieee.academic.internal.exception.AcademicConflictException;
import vn.edu.unieee.academic.internal.exception.AcademicResourceNotFoundException;
import vn.edu.unieee.academic.internal.mapper.AcademicMapper;
import vn.edu.unieee.academic.internal.repository.AcademicTermRepository;

@Service
@RequiredArgsConstructor
public class AcademicTermService {

    private final AcademicTermRepository termRepository;
    private final AcademicMapper mapper;
    private final ApplicationEventPublisher eventPublisher;

    @Transactional
    public AcademicTermResponse createTerm(CreateAcademicTermRequest request) {
        String code = request.code().trim();
        if (termRepository.existsByCode(code)) {
            throw new AcademicConflictException(
                "ACADEMIC_TERM_ALREADY_EXISTS",
                "Academic term already exists with code: " + code
            );
        }
        AcademicTerm term = AcademicTerm.create(code, request.name(), request.startDate(), request.endDate());
        return mapper.toTermResponse(termRepository.save(term));
    }

    @Transactional(readOnly = true)
    public AcademicTermResponse getTermById(UUID termId) {
        return mapper.toTermResponse(findTermOrThrow(termId));
    }

    @Transactional(readOnly = true)
    public AcademicTermResponse getTermByCode(String code) {
        AcademicTerm term = termRepository.findByCode(code.trim())
            .orElseThrow(() -> new AcademicResourceNotFoundException(
                "ACADEMIC_TERM_NOT_FOUND",
                "Academic term not found with code: " + code
            ));
        return mapper.toTermResponse(term);
    }

    @Transactional(readOnly = true)
    public List<AcademicTermResponse> listTerms() {
        return termRepository.findAllByOrderByStartDateDesc().stream()
            .map(mapper::toTermResponse)
            .toList();
    }

    @Transactional(readOnly = true)
    public boolean existsTerm(UUID termId) {
        return termRepository.existsById(termId);
    }

    @Transactional
    public AcademicTermResponse transitionStatus(UUID termId, TransitionAcademicTermStatusRequest request) {
        AcademicTerm term = findTermOrThrow(termId);
        AcademicTermStatus targetStatus = AcademicTermStatus.fromCode(request.targetStatus());
        AcademicTermStatus previousStatus = term.transitionTo(targetStatus);
        AcademicTerm saved = termRepository.save(term);

        eventPublisher.publishEvent(new AcademicTermStatusChangedEvent(
            saved.getId(),
            saved.getCode(),
            previousStatus.name(),
            saved.getStatus().name(),
            Instant.now()
        ));
        return mapper.toTermResponse(saved);
    }

    private AcademicTerm findTermOrThrow(UUID termId) {
        return termRepository.findById(termId)
            .orElseThrow(() -> new AcademicResourceNotFoundException(
                "ACADEMIC_TERM_NOT_FOUND",
                "Academic term not found: " + termId
            ));
    }
}
