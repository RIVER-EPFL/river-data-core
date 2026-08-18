# Source: cnet-data-portal (MIT, mclement18)
# File: app/modules/tools_tab/tools/discharge_tool.R
# Lines: 1-335
#
# Verbatim copy, do not modify. The Rust port lives in src/toolbox/discharge.rs.

## This module contains the UI and server code for the Discharge tool

## Create module UI function ######################################################

dischargeToolUI <- function(id, ...) {
# Create the UI for the dischargeTool module
# Parameters:
#  - id: String, the module id
# 
# Returns a div containing the layout
  
  # Create namespace
  ns <- NS(id)
  
  # Create layout
  div(
    class = 'discharge-tool tools-layout',
    div(
      class = 'file-upload',
      h4('Upload Data File'),
      fileInput(ns('dataFile'), 'Choose CSV File',
                accept = c('.csv')),
      div(
        class = 'file-preview',
        DT::dataTableOutput(ns('filePreview'))
      )
    ),
    div(
      class = 'parameters',
      h4('Parameters'),
      fluidRow(
        column(6,
          h5('Rhodamine Parameters'),
          numericInput(ns('initial_mass_rhodamine_wt'), 'Initial Mass Rhodamine (g)', 
                      value = 3.38019, min = 0, step = 0.01),
          numericInput(ns('concentration_rhodamine_wt'), 'Rhodamine Concentration (%)', 
                      value = 23.83, min = 0, max = 100, step = 0.01),
          numericInput(ns('initial_water_temp_degC'), 'Initial Water Temperature (°C)', 
                      value = 3.3, step = 0.1),
          numericInput(ns('n_rhodamine'), 'Temperature Correction Factor', 
                      value = 0.026, step = 0.001)
        ),
        column(6,
          h5('Salt Parameters'),
          numericInput(ns('initial_mass_salt'), 'Initial Mass Salt (g)', 
                      value = 2000, min = 0, step = 1),
          numericInput(ns('slope_conductivity'), 'Conductivity Slope', 
                      value = 1951.1, min = 0, step = 0.1)
        )
      ),
      fluidRow(
        column(6,
          numericInput(ns('distance'), 'Distance (m)', 
                      value = 79, min = 0, step = 1)
        ),
        column(6,
          numericInput(ns('T_ref'), 'Reference Temperature (°C)', 
                      value = 25, step = 1)
        )
      )
    ),
    div(
      class = 'calculation',
      div(
        class = 'calculation-header',
        h4('Calculate Discharge:'),
        actionButton(ns('calculate'), 'Calculate', class = 'custom-style custom-style--primary')
      ),
      div(
        class = 'results',
        h4('Results'),
        verbatimTextOutput(ns('results')),
        div(
          class = 'plots',
          plotOutput(ns('plots'), height = '400px')
        )
      )
    )
  )
}

## Create module server function ##################################################

dischargeTool <- function(input, output, session, pool, site, datetime, ...) {
# Create the logic for the dischargeTool module
# Parameters:
#  - input, output, session: Default needed parameters to create a module
#  - pool: The pool connection to the database
# 
# Returns a reactive expression containing the results
  
  ## Reactive values ##############################################################
  
  values <- reactiveValues(
    data = NULL,
    results = NULL,
    plots = NULL
  )
  
  ## File upload handling #########################################################
  
  observeEvent(input$dataFile, {
    req(input$dataFile)
    
    tryCatch({
      # Read the uploaded file
      data <- read.csv(input$dataFile$datapath, stringsAsFactors = FALSE)
      
      # Auto-detect time format and convert
      if ('time' %in% names(data)) {
        # Try different time formats
        time_formats <- c("%H:%M:%S", "%m/%d/%y %H:%M:%S", "%Y-%m-%d %H:%M:%S", "%d/%m/%Y %H:%M:%S")
        
        for (format in time_formats) {
          tryCatch({
            converted_time <- as.POSIXct(data$time, format = format)
            if (!all(is.na(converted_time))) {
              data$time <- converted_time
              showNotification(paste("Time format detected:", format), type = "message")
              break
            }
          }, error = function(e) {
            # Continue to next format
          })
        }
        
        # If no format worked, show warning
        if (all(is.na(data$time))) {
          showNotification("Could not detect time format. Please check your data.", type = "warning")
        }
      }
      
      values$data <- data
      
    }, error = function(e) {
      showNotification(paste("Error reading file:", e$message), type = "error")
    })
  })
  
  ## File preview output ##########################################################
  
  output$filePreview <- DT::renderDataTable({
    req(values$data)
    
    DT::datatable(
      head(values$data, 100),
      options = list(
        scrollX = TRUE,
        pageLength = 10,
        dom = 'tip'
      )
    )
  })
  
  ## Calculate discharge ###########################################################
  
  observeEvent(input$calculate, {
    req(values$data)
    
    tryCatch({
      data <- values$data
      
      # Get parameters from inputs
      initial_mass_rhodamine_wt <- input$initial_mass_rhodamine_wt
      concentration_rhodamine_wt <- input$concentration_rhodamine_wt / 100  # Convert percentage
      initial_water_temp_degC <- input$initial_water_temp_degC
      n_rhodamine <- input$n_rhodamine
      initial_mass_salt <- input$initial_mass_salt
      slope_conductivity <- input$slope_conductivity
      distance <- input$distance
      T_ref <- input$T_ref
      
      # Calculate initial mass of rhodamine
      initial_mass_rhodamine <- initial_mass_rhodamine_wt * concentration_rhodamine_wt * 1000
      
      # Initialize results
      results <- list()
      plots <- list()
      
      # Process Rhodamine data if available
      if ("ppb" %in% names(data)) {
        # Background correction
        background_indices <- c(1:min(15, nrow(data)), 
                               max(1, nrow(data)-9):nrow(data))
        background_model <- lm(ppb ~ as.numeric(time), data = data[background_indices, ])
        data$background_predicted <- predict(background_model, newdata = data)
        data$ppb_corrected <- data$ppb - data$background_predicted
        
        # Temperature correction
        data$ppb_temp_corrected <- data$ppb_corrected * exp(n_rhodamine * (initial_water_temp_degC - T_ref))
        
        # Smoothing
        data$ppb_smoothed <- sgolayfilt(data$ppb_temp_corrected, p = 3, n = min(11, nrow(data)))
        data$ppb_smoothed[data$ppb_smoothed < 0] <- 0
        
        # Calculate discharge
        time_seconds <- as.numeric(difftime(data$time, data$time[1], units = "secs"))
        auc_rhodamine <- trapz(time_seconds, data$ppb_smoothed)
        discharge_rhodamine <- initial_mass_rhodamine / (auc_rhodamine / 1000)
        
        # Calculate other metrics
        peak_concentration_rhodamine <- max(data$ppb_smoothed)
        peak_time_rhodamine <- data$time[which.max(data$ppb_smoothed)]
        travel_time_rhodamine <- as.numeric(difftime(peak_time_rhodamine, data$time[1], units = "secs"))
        velocity_rhodamine <- distance / travel_time_rhodamine
        
        # Store results
        results$rhodamine <- list(
          peak_concentration = peak_concentration_rhodamine,
          travel_time = travel_time_rhodamine,
          velocity = velocity_rhodamine,
          discharge = discharge_rhodamine
        )
        
        # Create plot
        plots$rhodamine <- ggplot(data, aes(x = time, y = ppb_smoothed)) +
          geom_line(color = "blue", size = 1) +
          geom_area(fill = "blue", alpha = 0.2) +
          labs(title = "Temperature-adjusted Rhodamine Concentration",
               x = "Time", y = "Concentration (ppb)") +
          theme_minimal()
      }
      
      # Process Salt data if available
      if ("uScm" %in% names(data)) {
        # Background correction
        background_indices <- c(1:min(10, nrow(data)), 
                               max(1, nrow(data)-9):nrow(data))
        background_model_salt <- lm(uScm ~ as.numeric(time), data = data[background_indices, ])
        data$background_predicted_salt <- predict(background_model_salt, newdata = data)
        data$uScm_corrected <- data$uScm - data$background_predicted_salt
        
        # Convert to concentration
        data$salt_concentration <- data$uScm_corrected / slope_conductivity
        
        # Smoothing
        data$salt_concentration_smoothed <- sgolayfilt(data$salt_concentration, p = 3, n = min(11, nrow(data)))
        data$salt_concentration_smoothed[data$salt_concentration_smoothed < 0] <- 0
        
        # Calculate discharge
        time_seconds <- as.numeric(difftime(data$time, data$time[1], units = "secs"))
        auc_salt <- trapz(time_seconds, data$salt_concentration_smoothed)
        discharge_salt <- initial_mass_salt / auc_salt
        
        # Calculate other metrics
        peak_concentration_salt <- max(data$salt_concentration_smoothed)
        peak_time_salt <- data$time[which.max(data$salt_concentration_smoothed)]
        travel_time_salt <- as.numeric(difftime(peak_time_salt, data$time[1], units = "secs"))
        velocity_salt <- distance / travel_time_salt
        
        # Store results
        results$salt <- list(
          peak_concentration = peak_concentration_salt,
          travel_time = travel_time_salt,
          velocity = velocity_salt,
          discharge = discharge_salt
        )
        
        # Create plot
        plots$salt <- ggplot(data, aes(x = time, y = salt_concentration_smoothed)) +
          geom_line(color = "red", size = 1) +
          geom_area(fill = "red", alpha = 0.2) +
          labs(title = "Salt Concentration",
               x = "Time", y = "Concentration (g/L)") +
          theme_minimal()
      }
      
      values$results <- results
      values$plots <- plots
      
      showNotification("Discharge calculation completed successfully!", type = "message")
      
    }, error = function(e) {
      showNotification(paste("Error in calculation:", e$message), type = "error")
    })
  })
  
  ## Results output ###############################################################
  
  output$results <- renderText({
    req(values$results)
    
    result_text <- ""
    
    if (!is.null(values$results$rhodamine)) {
      r <- values$results$rhodamine
      result_text <- paste0(result_text,
        "RHODAMINE RESULTS:\n",
        "Peak concentration: ", round(r$peak_concentration, 3), " ppb\n",
        "Travel time: ", round(r$travel_time, 2), " seconds\n",
        "Velocity: ", round(r$velocity, 4), " m/s\n",
        "Discharge: ", round(r$discharge, 4), " L/s\n\n"
      )
    }
    
    if (!is.null(values$results$salt)) {
      s <- values$results$salt
      result_text <- paste0(result_text,
        "SALT RESULTS:\n",
        "Peak concentration: ", round(s$peak_concentration, 4), " g/L\n",
        "Travel time: ", round(s$travel_time, 2), " seconds\n",
        "Velocity: ", round(s$velocity, 4), " m/s\n",
        "Discharge: ", round(s$discharge, 4), " L/s\n"
      )
    }
    
    if (result_text == "") {
      result_text <- "No valid tracer data found. Please ensure your file contains 'ppb' (rhodamine) or 'uScm' (conductivity) columns."
    }
    
    result_text
  })
  
  ## Plots output #################################################################
  
  output$plots <- renderPlot({
    req(values$plots)
    
    plot_list <- values$plots
    
    if (length(plot_list) == 2) {
      grid.arrange(plot_list$rhodamine, plot_list$salt, ncol = 2)
    } else if (!is.null(plot_list$rhodamine)) {
      plot_list$rhodamine
    } else if (!is.null(plot_list$salt)) {
      plot_list$salt
    }
  })
  
  ## Return reactive ##############################################################
  
  return(reactive({
    values$results
  }))
}
