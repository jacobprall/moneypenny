# == Schema Information
#
# Table name: budget_generators
#
#  id         :bigint           not null, primary key
#  created_at :datetime         not null
#  updated_at :datetime         not null
#  user_id    :integer          not null
#
class BudgetGenerator < ApplicationRecord
  # add risk category column?

#   BUDGET_TEMPLATE = {
#     risky: [0.5, 0,4, 0.1],
#     moderate: [0.6, 0.2, 0.2],
#     frugal: [0.4, 0.15, 0.45]
#   }
#  #HOW TO MODEL DATA?
#   SINGLE_STATE_TAX = {
#     "CA" => {0.1}
#     "NY" => {0.1: 12000, 0.2: 30000, 0.25: 55000, 0.3: 80000, 0.55: 180000}
#   }

  def annualAfterTaxIncome(responseObj)
  end


  
end
