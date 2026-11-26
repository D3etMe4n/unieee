package vn.edu.unieee.catalog.internal.service.term;

import java.time.Instant;
import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.context.ApplicationEventPublisher;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.catalog.dto.AcademicTermResponse;
import vn.edu.unieee.catalog.dto.CreateAcademicTermRequest;
import vn.edu.unieee.catalog.dto.TransitionAcademicTermStatusRequest;
import vn.edu.unieee.catalog.event.AcademicTermCreatedEvent;
import vn.edu.unieee.catalog.event.AcademicTermStatusChangedEvent;
import vn.edu.unieee.catalog.internal.domain.term.AcademicTerm;
import vn.edu.unieee.catalog.internal.domain.term.AcademicTermStatus;
import vn.edu.unieee.catalog.internal.exception.CatalogConflictException;
import vn.edu.unieee.catalog.internal.exception.CatalogResourceNotFoundException;
import vn.edu.unieee.catalog.internal.mapper.CatalogMapper;
import vn.edu.unieee.catalog.internal.repository.AcademicTermRepository;
import vn.edu.unieee.common.cache.ReadCacheProperties;
import vn.edu.unieee.common.cache.ReadThroughCache;
import vn.edu.unieee.common.cache.RedisCacheKeys;

@Service
@RequiredArgsConstructor
public class AcademicTermService {

    private final AcademicTermRepository termRepository;
    private final CatalogMapper mapper;
    private final ApplicationEventPublisher eventPublisher;
    private final ReadThroughCache readThroughCache;
    private final ReadCacheProperties readCacheProperties;

    @Transactional
    public AcademicTermResponse createTerm(CreateAcademicTermRequest request) {
        String code = request.code().trim();
        if (termRepository.existsByCode(code)) {
            throw new CatalogConflictException(
                "ACADEMIC_TERM_ALREADY_EXISTS",
                "Academic term already exists with code: " + code
            );
        }
        AcademicTerm saved = termRepository.save(
            AcademicTerm.create(code, request.name(), request.startDate(), request.endDate())
        );
        eventPublisher.publishEvent(new AcademicTermCreatedEvent(saved.getId(), saved.getCode(), Instant.now()));
        return mapper.toTermResponse(saved);
    }

    @Transactional(readOnly = true)
    public AcademicTermResponse getTermById(UUID termId) {
        return readThroughCache.getOrLoad(
            RedisCacheKeys.catalogTerm(termId),
            AcademicTermResponse.class,
            readCacheProperties.getCatalogTtl(),
            () -> mapper.toTermResponse(findTermOrThrow(termId))
        );
    }

    @Transactional(readOnly = true)
    public AcademicTermResponse getTermByIdAuthoritative(UUID termId) {
        return mapper.toTermResponse(findTermOrThrow(termId));
    }

    @Transactional(readOnly = true)
    public AcademicTermResponse getTermByCode(String code) {
        AcademicTerm term = termRepository.findByCode(code.trim())
            .orElseThrow(() -> new CatalogResourceNotFoundException(
                "ACADEMIC_TERM_NOT_FOUND",
                "Academic term not found with code: " + code
            ));
        return mapper.toTermResponse(term);
    }

    @Transactional(readOnly = true)
    public List<AcademicTermResponse> listTerms() {
        return readThroughCache.getOrLoadList(
            RedisCacheKeys.catalogTerms(),
            AcademicTermResponse.class,
            readCacheProperties.getCatalogTtl(),
            this::listTermsAuthoritative
        );
    }

    @Transactional(readOnly = true)
    public List<AcademicTermResponse> listTermsAuthoritative() {
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
            .orElseThrow(() -> new CatalogResourceNotFoundException(
                "ACADEMIC_TERM_NOT_FOUND",
                "Academic term not found: " + termId
            ));
    }
}
