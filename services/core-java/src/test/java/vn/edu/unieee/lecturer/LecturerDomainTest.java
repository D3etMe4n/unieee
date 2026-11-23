package vn.edu.unieee.lecturer;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import org.junit.jupiter.api.Test;
import vn.edu.unieee.lecturer.internal.domain.Lecturer;
import vn.edu.unieee.lecturer.internal.domain.LecturerStatus;
import vn.edu.unieee.lecturer.internal.exception.LecturerStateException;

class LecturerDomainTest {

    @Test
    void enforcesCreationInvariantsAndStatusTransitions() {
        assertThatThrownBy(() -> Lecturer.create("", "Dr. An", "an@unieee.edu.vn", "CS", "PhD"))
            .isInstanceOf(LecturerStateException.class);
        assertThatThrownBy(() -> Lecturer.create("GV01", "", "an@unieee.edu.vn", "CS", "PhD"))
            .isInstanceOf(LecturerStateException.class);
        assertThatThrownBy(() -> Lecturer.create("GV01", "Dr. An", "", "CS", "PhD"))
            .isInstanceOf(LecturerStateException.class);
        assertThatThrownBy(() -> Lecturer.create("GV01", "Dr. An", "an@unieee.edu.vn", "", "PhD"))
            .isInstanceOf(LecturerStateException.class);

        Lecturer lecturer = Lecturer.create("GV01", "Nguyen Van An", "AN@UNIEEE.EDU.VN", "CS", "  ");
        assertThat(lecturer.getEmail()).isEqualTo("an@unieee.edu.vn");
        assertThat(lecturer.getAcademicTitle()).isNull();
        assertThat(lecturer.isAccountEnabled()).isTrue();
        assertThat(lecturer.isAssignableToSection()).isTrue();

        lecturer.linkKeycloakAccount("kc-sub-gv01");
        assertThat(lecturer.getKeycloakSub()).isEqualTo("kc-sub-gv01");
        assertThatThrownBy(() -> lecturer.linkKeycloakAccount(" "))
            .isInstanceOf(LecturerStateException.class);

        lecturer.updateProfile("Assoc. Prof. Nguyen Van An", "SE", "Associate Professor");
        assertThat(lecturer.getDepartmentCode()).isEqualTo("SE");
        assertThat(lecturer.getAcademicTitle()).isEqualTo("Associate Professor");
        assertThatThrownBy(() -> lecturer.updateProfile("", "SE", null))
            .isInstanceOf(LecturerStateException.class);

        lecturer.changeStatus(LecturerStatus.ON_LEAVE);
        assertThat(lecturer.isAccountEnabled()).isTrue();
        assertThat(lecturer.isAssignableToSection()).isFalse();

        lecturer.changeStatus(LecturerStatus.LOCKED);
        assertThat(lecturer.isAccountEnabled()).isFalse();

        lecturer.changeStatus(LecturerStatus.RESIGNED);
        lecturer.changeStatus(LecturerStatus.RESIGNED);
        assertThatThrownBy(() -> lecturer.changeStatus(LecturerStatus.ACTIVE))
            .isInstanceOf(LecturerStateException.class);
        assertThatThrownBy(() -> lecturer.changeStatus(null))
            .isInstanceOf(LecturerStateException.class);

        assertThat(LecturerStatus.fromCode("on_leave")).isEqualTo(LecturerStatus.ON_LEAVE);
        assertThatThrownBy(() -> LecturerStatus.fromCode("")).isInstanceOf(LecturerStateException.class);
        assertThatThrownBy(() -> LecturerStatus.fromCode("RETIRED")).isInstanceOf(LecturerStateException.class);
    }
}
