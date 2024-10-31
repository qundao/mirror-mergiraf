package org.springframework.boot.autoconfigure;

import java.util.List;
import org.junit.Test;
import org.springframework.boot.autoconfigure.AutoConfigurationPackages.Registrar;
import org.springframework.boot.autoconfigure.packagestest.one.FirstConfiguration;
import org.springframework.boot.autoconfigure.packagestest.two.SecondConfiguration;
import org.springframework.context.annotation.AnnotationConfigApplicationContext;
import org.springframework.context.annotation.Configuration;
import org.springframework.context.annotation.Import;
import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatIllegalStateException;

@SuppressWarnings("resource")
public class AutoConfigurationPackagesTests {

    @Test
    public void setAndGet() {
        AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(ConfigWithRegistrar.class);
        assertThat(AutoConfigurationPackages.get(context.getBeanFactory())).containsExactly(getClass().getPackage().getName());
    }

    @Test
    public void getWithoutSet() {
        AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(EmptyConfig.class);
        assertThatIllegalStateException().isThrownBy(() -> AutoConfigurationPackages.get(context.getBeanFactory())).withMessageContaining("Unable to retrieve @EnableAutoConfiguration base packages");
    }

    @Test
    public void detectsMultipleAutoConfigurationPackages() {
        AnnotationConfigApplicationContext context = new AnnotationConfigApplicationContext(FirstConfiguration.class, SecondConfiguration.class);
        List<String> packages = AutoConfigurationPackages.get(context.getBeanFactory());
        Package package1 = FirstConfiguration.class.getPackage();
        Package package2 = SecondConfiguration.class.getPackage();
        assertThat(packages).containsOnly(package1.getName(), package2.getName());
    }

    @Configuration
    @Import(AutoConfigurationPackages.Registrar.class)
    static class ConfigWithRegistrar {
    }

    @Configuration
    static class EmptyConfig {
    }

    public static class TestRegistrar extends Registrar {
    }
}
