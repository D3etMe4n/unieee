package vn.edu.unieee.registration.internal.domain.policy;

import jakarta.persistence.AttributeConverter;
import jakarta.persistence.Converter;

@Converter
public class GradeRankAttributeConverter implements AttributeConverter<GradeRank, String> {

    @Override
    public String convertToDatabaseColumn(GradeRank attribute) {
        return attribute == null ? null : attribute.getCode();
    }

    @Override
    public GradeRank convertToEntityAttribute(String dbData) {
        return dbData == null ? null : GradeRank.fromCode(dbData);
    }
}
