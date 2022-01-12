mod utils;

use argmin::prelude::{ArgminKV, Error, IterState, Observe};
use serde::Serialize;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

use keyboard_layout::{
    config::LayoutConfig, keyboard::Keyboard, layout::Layout, layout_generator::NeoLayoutGenerator,
};

use layout_evaluation::{
    config::EvaluationParameters,
    evaluation::Evaluator,
    ngram_mapper::on_demand_ngram_mapper::OnDemandNgramMapper,
    ngrams::{Bigrams, Trigrams, Unigrams},
    results::EvaluationResult,
};

use layout_optimization::common::{Cache, PermutationLayoutGenerator};
use layout_optimization_genevo::optimization as gen_optimization;
use layout_optimization_sa::optimization as sa_optimization;

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[derive(Debug, Clone, Serialize)]
struct LayoutEvaluation {
    total_cost: f64,
    details: EvaluationResult,
    printed: Option<String>,
    plot: Option<String>,
    layout: Option<String>,
}

impl From<EvaluationResult> for LayoutEvaluation {
    fn from(res: EvaluationResult) -> Self {
        Self {
            total_cost: res.total_cost(),
            details: res.clone(),
            printed: None,
            plot: None,
            layout: None,
        }
    }
}

#[wasm_bindgen]
pub struct LayoutPlotter {
    layout_generator: NeoLayoutGenerator,
}

#[wasm_bindgen]
impl LayoutPlotter {
    pub fn new(layout_cfg_str: &str) -> Result<LayoutPlotter, JsValue> {
        utils::set_panic_hook();

        let layout_cfg = LayoutConfig::from_str(layout_cfg_str)
            .map_err(|e| format!("Could not read layout config: {:?}", e))?;

        let keyboard = Arc::new(Keyboard::from_yaml_object(layout_cfg.keyboard));

        let layout_generator =
            NeoLayoutGenerator::from_object(layout_cfg.base_layout, keyboard.clone());

        Ok(LayoutPlotter { layout_generator })
    }

    pub fn plot(&self, layout_str: &str, layer: usize) -> Result<String, JsValue> {
        let layout = self
            .layout_generator
            .generate_unchecked(layout_str)
            .map_err(|e| format!("Could not plot the layout: {:?}", e))?;
        Ok(layout.plot_layer(layer))
    }
}

#[wasm_bindgen]
pub struct NgramProvider {
    ngram_provider: OnDemandNgramMapper,
}

#[wasm_bindgen]
impl NgramProvider {
    pub fn with_frequencies(
        eval_params_str: &str,
        unigrams_str: &str,
        bigrams_str: &str,
        trigrams_str: &str,
    ) -> Result<NgramProvider, JsValue> {
        let unigrams = Unigrams::from_frequencies_str(unigrams_str)
            .map_err(|e| format!("Could not load unigrams: {:?}", e))?;

        let bigrams = Bigrams::from_frequencies_str(bigrams_str)
            .map_err(|e| format!("Could not load bigrams: {:?}", e))?;

        let trigrams = Trigrams::from_frequencies_str(trigrams_str)
            .map_err(|e| format!("Could not load trigrams: {:?}", e))?;

        let eval_params: EvaluationParameters = serde_yaml::from_str(eval_params_str)
            .map_err(|e| format!("Could not read evaluation parameters: {:?}", e))?;

        let ngram_mapper_config = eval_params.ngram_mapper.clone();

        let ngram_provider =
            OnDemandNgramMapper::with_ngrams(unigrams, bigrams, trigrams, ngram_mapper_config);

        Ok(NgramProvider { ngram_provider })
    }

    pub fn with_text(eval_params_str: &str, text: &str) -> Result<NgramProvider, JsValue> {
        let eval_params: EvaluationParameters = serde_yaml::from_str(eval_params_str)
            .map_err(|e| format!("Could not read evaluation parameters: {:?}", e))?;

        let ngram_mapper_config = eval_params.ngram_mapper.clone();

        let ngram_provider = OnDemandNgramMapper::with_corpus(&text, ngram_mapper_config);

        Ok(NgramProvider { ngram_provider })
    }
}

#[wasm_bindgen]
pub struct LayoutEvaluator {
    layout_generator: NeoLayoutGenerator,
    evaluator: Evaluator,
}

#[wasm_bindgen]
impl LayoutEvaluator {
    pub fn new(
        layout_cfg_str: &str,
        eval_params_str: &str,
        ngram_provider: &NgramProvider,
    ) -> Result<LayoutEvaluator, JsValue> {
        utils::set_panic_hook();

        let layout_cfg = LayoutConfig::from_str(layout_cfg_str)
            .map_err(|e| format!("Could not read layout config: {:?}", e))?;

        let keyboard = Arc::new(Keyboard::from_yaml_object(layout_cfg.keyboard));

        let layout_generator =
            NeoLayoutGenerator::from_object(layout_cfg.base_layout, keyboard.clone());

        let eval_params: EvaluationParameters = serde_yaml::from_str(eval_params_str)
            .map_err(|e| format!("Could not read evaluation parameters: {:?}", e))?;

        let evaluator = Evaluator::default(Box::new(ngram_provider.ngram_provider.clone()))
            .default_metrics(&eval_params.metrics);

        Ok(LayoutEvaluator {
            layout_generator,
            evaluator,
        })
    }

    pub fn evaluate(&self, layout_str: &str) -> Result<JsValue, JsValue> {
        let layout = self
            .layout_generator
            .generate(layout_str)
            .map_err(|e| format!("Could not generate layout: {:?}", e))?;
        let res = self.evaluator.evaluate_layout(&layout);
        let printed = Some(format!("{}", res));
        let plot = Some(layout.plot());
        let layout_str = Some(layout.as_text());

        let mut res: LayoutEvaluation = res.into();
        res.printed = printed;
        res.plot = plot;
        res.layout = layout_str;
        Ok(JsValue::from_serde(&res).unwrap())
    }

    pub fn plot(&self, layout_str: &str, layer: usize) -> Result<String, JsValue> {
        let layout = self
            .layout_generator
            .generate(layout_str)
            .map_err(|e| format!("Could not plot the layout: {:?}", e))?;
        Ok(layout.plot_layer(layer))
    }

    pub fn permutable_keys(&self) -> JsValue {
        let permutable_keys = self.layout_generator.permutable_keys();
        return JsValue::from_serde(&permutable_keys).unwrap();
    }
}

#[wasm_bindgen]
pub struct LayoutOptimizer {
    evaluator: Evaluator,
    simulator: gen_optimization::MySimulator,
    permutation_layout_generator: PermutationLayoutGenerator,
    all_time_best: Option<(usize, Vec<usize>)>,
    parameters: gen_optimization::Parameters,
}

#[wasm_bindgen]
impl LayoutOptimizer {
    pub fn new(
        layout_str: &str,
        optimization_params_str: &str,
        layout_evaluator: &LayoutEvaluator,
        fixed_characters: &str,
        start_with_layout: bool,
    ) -> Result<LayoutOptimizer, JsValue> {
        utils::set_panic_hook();

        let parameters: gen_optimization::Parameters =
            serde_yaml::from_str(optimization_params_str)
                .map_err(|e| format!("Could not read optimization params: {:?}", e))?;

        let (simulator, permutation_layout_generator) = gen_optimization::init_optimization(
            &parameters,
            &layout_evaluator.evaluator,
            layout_str,
            &layout_evaluator.layout_generator,
            fixed_characters,
            start_with_layout,
            true,
        );

        Ok(LayoutOptimizer {
            evaluator: layout_evaluator.evaluator.clone(),
            simulator,
            permutation_layout_generator,
            all_time_best: None,
            parameters,
        })
    }

    pub fn parameters(&self) -> JsValue {
        return JsValue::from_serde(&self.parameters).unwrap();
    }

    pub fn step(&mut self) -> Result<JsValue, JsValue> {
        use genevo::prelude::*;

        let result = self.simulator.step();
        match result {
            Ok(SimResult::Intermediate(step)) => {
                let best_solution = step.result.best_solution;
                if let Some(king) = &self.all_time_best {
                    if best_solution.solution.fitness > king.0 {
                        self.all_time_best = Some((
                            best_solution.solution.fitness,
                            best_solution.solution.genome.clone(),
                        ));
                    }
                } else {
                    self.all_time_best = Some((
                        best_solution.solution.fitness,
                        best_solution.solution.genome.clone(),
                    ));
                }

                let layout = self
                    .permutation_layout_generator
                    .generate_layout(&self.all_time_best.as_ref().unwrap().1);
                let res = self.evaluator.evaluate_layout(&layout);
                let printed = Some(format!("{}", res));
                let plot = Some(layout.plot());
                let layout_str = Some(layout.as_text());

                let mut res: LayoutEvaluation = res.into();
                res.printed = printed;
                res.plot = plot;
                res.layout = layout_str;

                return Ok(JsValue::from_serde(&Some(res)).unwrap());
            }
            Ok(SimResult::Final(_, _, _, _)) => {
                return Ok(JsValue::from_serde(&None::<Option<EvaluationResult>>).unwrap());
                // break
            }
            Err(error) => {
                return Err(format!("Error in optimization: {:?}", error))?;
                // break
            }
        }
    }
}

/// An observer that outputs important information in a more human-readable format than `Argmin`'s original implementation.
struct SaObserver {
    layout_generator: PermutationLayoutGenerator,
    update_callback: js_sys::Function,
    new_best_callback: js_sys::Function,
}

impl Observe<sa_optimization::AnnealingStruct> for SaObserver {
    fn observe_iter(
        &mut self,
        state: &IterState<sa_optimization::AnnealingStruct>,
        _kv: &ArgminKV,
    ) -> Result<(), Error> {
        let iteration_nr = state.iter;
        if (iteration_nr % 10 == 0) && (iteration_nr > 0) {
            let this = JsValue::null();
            let iter_js = JsValue::from(iteration_nr);
            let _ = self.update_callback.call1(&this, &iter_js);
        }
        if state.is_best() {
            let this = JsValue::null();
            let layout_js = JsValue::from(self.layout_generator.generate_string(&state.param));
            let cost_js = JsValue::from(state.cost);
            let _ = self.new_best_callback.call2(&this, &layout_js, &cost_js);
        }
        Ok(())
    }
}

#[wasm_bindgen]
pub fn sa_optimize(
    layout_str: &str,
    optimization_params_str: &str,
    layout_evaluator: &LayoutEvaluator,
    fixed_characters: &str,
    start_with_layout: bool,
    max_iters_callback: js_sys::Function,
    update_callback: js_sys::Function,
    new_best_callback: js_sys::Function,
) -> String {
    let mut parameters: sa_optimization::Parameters = serde_yaml::from_str(optimization_params_str)
        .map_err(|e| format!("Could not read optimization params: {:?}", e))
        .unwrap();
    // Make sure the initial temperature is greater than zero.
    parameters.correct_init_temp();

    // Display the maximum amount of iterations on the website.
    let this = JsValue::null();
    let max_iters_js = JsValue::from(parameters.max_iters);
    let _ = max_iters_callback.call1(&this, &max_iters_js);
    let one = JsValue::from(1);
    let _ = update_callback.call1(&this, &one);

    let observer = SaObserver {
        layout_generator: PermutationLayoutGenerator::new(
            layout_str,
            fixed_characters,
            &layout_evaluator.layout_generator,
        ),
        update_callback,
        new_best_callback,
    };

    let result: Layout = sa_optimization::optimize(
        /* Thread_name: */ "Web optimization",
        &parameters,
        layout_str,
        fixed_characters,
        &layout_evaluator.layout_generator,
        start_with_layout,
        &layout_evaluator.evaluator,
        /* log_everything: */ false,
        Some(Cache::new()),
        Some(Box::new(observer)),
    );
    result.as_text()
}
