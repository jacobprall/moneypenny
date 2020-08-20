# == Schema Information
#
# Table name: transactions
#
#  id                   :bigint           not null, primary key
#  amount               :float            not null
#  date                 :string
#  description          :string           not null
#  tags                 :string
#  transaction_category :string           not null
#  created_at           :datetime         not null
#  updated_at           :datetime         not null
#  account_id           :integer          not null
#
class Transaction < ApplicationRecord
  validates_presence_of :amount, :date, :description, :transaction_category, :account_id
  validates :transaction_category, inclusion: { in: %w(Housing Transportation Food Utilities Healthcare Personal Recreation/Entertainment Shopping Miscellaneous Other)}
  belongs_to :account
end
