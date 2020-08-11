# == Schema Information
#
# Table name: accounts
#
#  id            :bigint           not null, primary key
#  balance       :decimal(8, 2)    not null
#  balance_sheet :string           not null
#  category      :string           not null
#  inst          :string
#  label         :string           not null
#  created_at    :datetime         not null
#  updated_at    :datetime         not null
#  user_id       :string           not null
#
# Indexes
#
#  index_accounts_on_user_id  (user_id)
#

ACCOUNT_TYPES = [
  "Cash",
  "Credit Cards",
  "Loans",
  "Investments",
  "Property"
]



INSTITUTIONS = [
  "J.P. Morgan Chase",
  "Bank of America",
  "Wells Fargo",
  "Citi"
]

require 'date'
class Account < ApplicationRecord

  validates :account_type, presence: true, inclusion: { in: ACCOUNT_TYPES }
  validates :balance, :inst, :label, presence: true
  validates :balance_sheet, inclusion: { in: %w(Asset Liability)}
  
  belongs_to :user,
  foreign_key: :user_id,
  class_name: :User 
  
  has_many :transactions,
  foreign_key: :account_id,
  class_name: :Transaction 

  has_many :goals,
  foreign_key: :account_id,
  class_name: :Goal
  
  def add_inst(inst)
    if !INSTITUTIONS.include?(inst)
      INSTITUTIONS.push(inst);
    end
  end


end
