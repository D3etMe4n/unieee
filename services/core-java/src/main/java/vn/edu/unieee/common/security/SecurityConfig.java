package vn.edu.unieee.common.security;

import jakarta.servlet.http.HttpServletResponse;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.time.Instant;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.http.HttpStatus;
import org.springframework.http.MediaType;
import org.springframework.security.config.annotation.method.configuration.EnableMethodSecurity;
import org.springframework.security.config.annotation.web.builders.HttpSecurity;
import org.springframework.security.config.annotation.web.configuration.EnableWebSecurity;
import org.springframework.security.config.annotation.web.configurers.AbstractHttpConfigurer;
import org.springframework.security.config.http.SessionCreationPolicy;
import org.springframework.security.web.SecurityFilterChain;

@Configuration
@EnableWebSecurity
@EnableMethodSecurity
public class SecurityConfig {

    private final KeycloakJwtAuthenticationConverter jwtAuthenticationConverter;

    public SecurityConfig(KeycloakJwtAuthenticationConverter jwtAuthenticationConverter) {
        this.jwtAuthenticationConverter = jwtAuthenticationConverter;
    }

    @Bean
    public SecurityFilterChain securityFilterChain(HttpSecurity http) throws Exception {
        return http
            .csrf(AbstractHttpConfigurer::disable)
            .sessionManagement(sm -> sm.sessionCreationPolicy(SessionCreationPolicy.STATELESS))
            .authorizeHttpRequests(auth -> auth
                .requestMatchers("/actuator/health/**", "/actuator/info").permitAll()
                .requestMatchers("/api/**").authenticated()
                .anyRequest().authenticated()
            )
            .oauth2ResourceServer(oauth2 -> oauth2
                .jwt(jwt -> jwt.jwtAuthenticationConverter(jwtAuthenticationConverter))
                .authenticationEntryPoint((request, response, authException) ->
                    writeProblemDetail(
                        response,
                        HttpStatus.UNAUTHORIZED,
                        "UNAUTHORIZED",
                        "Authentication is required to access this resource",
                        request.getRequestURI()
                    )
                )
            )
            .exceptionHandling(ex -> ex
                .authenticationEntryPoint((request, response, authException) ->
                    writeProblemDetail(
                        response,
                        HttpStatus.UNAUTHORIZED,
                        "UNAUTHORIZED",
                        "Authentication is required to access this resource",
                        request.getRequestURI()
                    )
                )
                .accessDeniedHandler((request, response, accessDeniedException) ->
                    writeProblemDetail(
                        response,
                        HttpStatus.FORBIDDEN,
                        "FORBIDDEN",
                        "You do not have permission to access this resource",
                        request.getRequestURI()
                    )
                )
            )
            .build();
    }

    private static void writeProblemDetail(
        HttpServletResponse response,
        HttpStatus status,
        String errorCode,
        String detail,
        String instance
    ) throws IOException {
        response.setStatus(status.value());
        response.setCharacterEncoding(StandardCharsets.UTF_8.name());
        response.setContentType(MediaType.APPLICATION_PROBLEM_JSON_VALUE);
        String safeInstance = instance == null ? "" : instance.replace("\"", "\\\"");
        String body = """
            {"type":"urn:unieee:error:%s","title":"%s","status":%d,"detail":"%s","instance":"%s","errorCode":"%s","timestamp":"%s"}"""
            .formatted(
                errorCode,
                status.getReasonPhrase(),
                status.value(),
                detail,
                safeInstance,
                errorCode,
                Instant.now().toString()
            );
        response.getWriter().write(body);
    }
}
