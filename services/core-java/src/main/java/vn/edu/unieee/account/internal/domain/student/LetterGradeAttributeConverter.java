package vn.edu.unieee.account.internal.domain.student;

import jakarta.persistence.AttributeConverter;
import jakarta.persistence.Converter;

@Converter
public class LetterGradeAttributeConverter implements AttributeConverter<LetterGrade, String> {

    @Override
    public String convertToDatabaseColumn(LetterGrade attribute) {
        return attribute == null ? null : attribute.getCode();
    }

    @Override
    public LetterGrade convertToEntityAttribute(String dbData) {
        return dbData == null ? null : LetterGrade.fromCode(dbData);
    }
}
